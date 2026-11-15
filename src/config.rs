use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedFilterConfig {
    #[serde(default)]
    pub include_keywords: Vec<String>,
    #[serde(default)]
    pub exclude_keywords: Vec<String>,
    #[serde(default)]
    pub include_regex: Vec<String>,
    #[serde(default)]
    pub exclude_regex: Vec<String>,
    #[serde(default = "default_true")]
    pub clean_links: bool,
    #[serde(default)]
    pub max_age_days: Option<i64>,
    #[serde(default)]
    pub max_items: Option<usize>,
    #[serde(default)]
    pub scripts: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedConfig {
    pub url: String,
    pub title_override: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub filters: FeedFilterConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: u64,
    #[serde(default = "default_plugins_dir")]
    pub plugins_dir: String,
    #[serde(default)]
    pub feeds: HashMap<String, FeedConfig>,
}

fn default_cache_ttl() -> u64 {
    300
}

fn default_plugins_dir() -> String {
    "./plugins".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            cache_ttl_seconds: default_cache_ttl(),
            plugins_dir: default_plugins_dir(),
            feeds: HashMap::new(),
        }
    }
}

impl AppConfig {
    pub fn load_or_default<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let p = path.as_ref();
        if !p.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(p)
            .map_err(|e| anyhow::anyhow!("Failed to read config file {:?}: {}", p, e))?;

        let config: AppConfig = serde_yaml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse YAML config: {}", e))?;

        Ok(config)
    }
}
