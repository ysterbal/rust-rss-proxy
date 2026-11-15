pub mod builtin;
pub mod rhai_plugin;

use crate::model::{Feed, FeedItem};
use async_trait::async_trait;
use tracing::{debug, info};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterDecision {
    Keep,
    Drop,
}

#[async_trait]
pub trait Plugin: Send + Sync {
    /// Human-readable plugin name
    fn name(&self) -> &str;

    /// Feed-level pre-processing (executed before item filtering)
    async fn pre_process_feed(&self, _feed: &mut Feed) -> anyhow::Result<()> {
        Ok(())
    }

    /// Evaluates whether an item should be kept or dropped
    async fn filter_item(&self, _item: &FeedItem) -> anyhow::Result<FilterDecision> {
        Ok(FilterDecision::Keep)
    }

    /// Transforms/mutates an item in-place (e.g., cleans URLs, modifies title, alters content)
    async fn transform_item(&self, _item: &mut FeedItem) -> anyhow::Result<()> {
        Ok(())
    }

    /// Feed-level post-processing (e.g. sorting, item count limiting)
    async fn post_process_feed(&self, _feed: &mut Feed) -> anyhow::Result<()> {
        Ok(())
    }

    /// Invoked when upstream feed fails to fetch or parse. Returns an optional fallback Feed (e.g., NOP feed)
    async fn on_fetch_error(&self, _url: &str, _title: &str, _error: &anyhow::Error) -> Option<Feed> {
        None
    }
}

pub struct PluginPipeline {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginPipeline {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn add_plugin(&mut self, plugin: Box<dyn Plugin>) {
        info!("Registered plugin: {}", plugin.name());
        self.plugins.push(plugin);
    }

    /// Handle upstream fetch errors by delegating to plugins that implement on_fetch_error
    pub async fn handle_fetch_error(&self, url: &str, title: &str, error: &anyhow::Error) -> Option<Feed> {
        for plugin in &self.plugins {
            if let Some(fallback) = plugin.on_fetch_error(url, title, error).await {
                return Some(fallback);
            }
        }
        None
    }

    /// Process a feed through all stages of the pipeline:
    /// 1. pre_process_feed
    /// 2. filter_item (dropping rejected items)
    /// 3. transform_item (mutating remaining items)
    /// 4. post_process_feed
    pub async fn process(&self, feed: &mut Feed) -> anyhow::Result<()> {
        // Stage 1: Pre-process feed
        for plugin in &self.plugins {
            plugin.pre_process_feed(feed).await?;
        }

        // Stage 2 & 3: Filter & Transform items
        let mut filtered_items = Vec::with_capacity(feed.items.len());

        for mut item in std::mem::take(&mut feed.items) {
            let mut keep = true;

            for plugin in &self.plugins {
                match plugin.filter_item(&item).await {
                    Ok(FilterDecision::Keep) => {}
                    Ok(FilterDecision::Drop) => {
                        debug!(
                            "Plugin '{}' dropped item '{}'",
                            plugin.name(),
                            item.title
                        );
                        keep = false;
                        break;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Plugin '{}' error filtering item '{}': {}. Defaulting to keep.",
                            plugin.name(),
                            item.title,
                            e
                        );
                    }
                }
            }

            if keep {
                for plugin in &self.plugins {
                    if let Err(e) = plugin.transform_item(&mut item).await {
                        tracing::warn!(
                            "Plugin '{}' error transforming item '{}': {}",
                            plugin.name(),
                            item.title,
                            e
                        );
                    }
                }
                filtered_items.push(item);
            }
        }

        feed.items = filtered_items;

        // Stage 4: Post-process feed
        for plugin in &self.plugins {
            plugin.post_process_feed(feed).await?;
        }

        Ok(())
    }
}
