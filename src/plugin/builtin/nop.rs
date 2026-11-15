use crate::model::Feed;
use crate::plugin::Plugin;
use async_trait::async_trait;
use tracing::warn;

pub struct NopHandlerPlugin;

impl NopHandlerPlugin {
    pub fn new() -> Self {
        Self
    }

    pub fn make_nop_feed(url: &str, title: &str, reason: &str) -> Feed {
        Feed {
            title: format!("{} [NOP]", title),
            link: url.to_string(),
            description: format!("Feed marked as NOP: {}", reason),
            language: Some("en".to_string()),
            items: Vec::new(),
        }
    }
}

impl Default for NopHandlerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for NopHandlerPlugin {
    fn name(&self) -> &str {
        "NopHandlerPlugin"
    }

    async fn on_fetch_error(&self, url: &str, title: &str, error: &anyhow::Error) -> Option<Feed> {
        let reason = error.to_string();
        warn!("Feed '{}' failed upstream fetch ({}). Serving NOP fallback feed.", title, reason);
        Some(Self::make_nop_feed(url, title, &reason))
    }
}
