use crate::model::{Feed, FeedItem};
use crate::plugin::{FilterDecision, Plugin};
use async_trait::async_trait;
use rhai::{Dynamic, Engine, Scope, AST};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

pub struct RhaiPlugin {
    name: String,
    path: PathBuf,
    engine: Engine,
    ast: AST,
}

impl RhaiPlugin {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed_plugin")
            .to_string();

        let engine = Engine::new();
        let ast = engine
            .compile_file(path.clone())
            .map_err(|e| anyhow::anyhow!("Failed to compile Rhai script {:?}: {}", path, e))?;

        Ok(Self {
            name,
            path,
            engine,
            ast,
        })
    }

    /// Load all *.rhai files in a folder
    pub fn load_directory<P: AsRef<Path>>(dir: P) -> Vec<Self> {
        let mut plugins = Vec::new();
        let dir = dir.as_ref();

        if !dir.exists() || !dir.is_dir() {
            return plugins;
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().map_or(false, |ext| ext == "rhai") {
                    match Self::from_file(&p) {
                        Ok(plugin) => plugins.push(plugin),
                        Err(e) => warn!("Error loading Rhai plugin at {:?}: {}", p, e),
                    }
                }
            }
        }

        plugins
    }
}

#[async_trait]
impl Plugin for RhaiPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    async fn filter_item(&self, item: &FeedItem) -> anyhow::Result<FilterDecision> {
        let mut scope = Scope::new();
        let dynamic_item = rhai::serde::to_dynamic(item)
            .map_err(|e| anyhow::anyhow!("Failed to serialize FeedItem to Rhai: {}", e))?;

        // Call filter_item(item)
        let result =
            self.engine
                .call_fn::<Dynamic>(&mut scope, &self.ast, "filter_item", (dynamic_item,));

        match result {
            Ok(val) => {
                // If returns a boolean: true -> Keep, false -> Drop
                if let Some(b) = val.clone().try_cast::<bool>() {
                    return Ok(if b {
                        FilterDecision::Keep
                    } else {
                        FilterDecision::Drop
                    });
                }

                // If returns a string: "keep" / "drop"
                if let Some(s) = val.clone().try_cast::<String>() {
                    let s = s.trim().to_lowercase();
                    if s == "drop" || s == "reject" {
                        return Ok(FilterDecision::Drop);
                    } else {
                        return Ok(FilterDecision::Keep);
                    }
                }

                if let Some(s) = val.clone().try_cast::<rhai::ImmutableString>() {
                    let s = s.trim().to_lowercase();
                    if s == "drop" || s == "reject" {
                        return Ok(FilterDecision::Drop);
                    } else {
                        return Ok(FilterDecision::Keep);
                    }
                }

                Ok(FilterDecision::Keep)
            }
            Err(e) => match *e {
                // Hook not defined in script is completely normal
                rhai::EvalAltResult::ErrorFunctionNotFound(ref name, _)
                    if name.starts_with("filter_item") =>
                {
                    Ok(FilterDecision::Keep)
                }
                other => {
                    warn!(
                        "Rhai plugin '{}' error in filter_item: {}",
                        self.name, other
                    );
                    Ok(FilterDecision::Keep)
                }
            },
        }
    }

    async fn transform_item(&self, item: &mut FeedItem) -> anyhow::Result<()> {
        let mut scope = Scope::new();
        let dynamic_item = rhai::serde::to_dynamic(&*item)
            .map_err(|e| anyhow::anyhow!("Failed to serialize FeedItem to Rhai: {}", e))?;

        // Call transform_item(item)
        let result = self.engine.call_fn::<Dynamic>(
            &mut scope,
            &self.ast,
            "transform_item",
            (dynamic_item,),
        );

        match result {
            Ok(val) => {
                if let Ok(updated_item) = rhai::serde::from_dynamic::<FeedItem>(&val) {
                    *item = updated_item;
                    debug!("Rhai plugin '{}' updated item '{}'", self.name, item.title);
                }
            }
            Err(e) => match *e {
                rhai::EvalAltResult::ErrorFunctionNotFound(ref name, _)
                    if name.starts_with("transform_item") =>
                {
                    // Hook not defined, nothing to do
                }
                other => {
                    warn!(
                        "Rhai plugin '{}' error in transform_item: {}",
                        self.name, other
                    );
                }
            },
        }

        Ok(())
    }

    async fn pre_process_feed(&self, feed: &mut Feed) -> anyhow::Result<()> {
        let mut scope = Scope::new();
        let dynamic_feed = rhai::serde::to_dynamic(&*feed)
            .map_err(|e| anyhow::anyhow!("Failed to serialize Feed to Rhai: {}", e))?;

        let result = self.engine.call_fn::<Dynamic>(
            &mut scope,
            &self.ast,
            "pre_process_feed",
            (dynamic_feed,),
        );

        match result {
            Ok(val) => {
                if let Ok(updated_feed) = rhai::serde::from_dynamic::<Feed>(&val) {
                    *feed = updated_feed;
                }
            }
            Err(e) => match *e {
                rhai::EvalAltResult::ErrorFunctionNotFound(ref name, _)
                    if name.starts_with("pre_process_feed") => {}
                other => {
                    warn!(
                        "Rhai plugin '{}' error in pre_process_feed: {}",
                        self.name, other
                    );
                }
            },
        }

        Ok(())
    }

    async fn post_process_feed(&self, feed: &mut Feed) -> anyhow::Result<()> {
        let mut scope = Scope::new();
        let dynamic_feed = rhai::serde::to_dynamic(&*feed)
            .map_err(|e| anyhow::anyhow!("Failed to serialize Feed to Rhai: {}", e))?;

        let result = self.engine.call_fn::<Dynamic>(
            &mut scope,
            &self.ast,
            "post_process_feed",
            (dynamic_feed,),
        );

        match result {
            Ok(val) => {
                if let Ok(updated_feed) = rhai::serde::from_dynamic::<Feed>(&val) {
                    *feed = updated_feed;
                }
            }
            Err(e) => match *e {
                rhai::EvalAltResult::ErrorFunctionNotFound(ref name, _)
                    if name.starts_with("post_process_feed") => {}
                other => {
                    warn!(
                        "Rhai plugin '{}' error in post_process_feed: {}",
                        self.name, other
                    );
                }
            },
        }

        Ok(())
    }
}
