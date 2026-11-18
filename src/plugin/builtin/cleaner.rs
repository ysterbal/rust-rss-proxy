use crate::model::FeedItem;
use crate::plugin::{FilterDecision, Plugin};
use async_trait::async_trait;

pub struct FeedCleanerPlugin;

impl FeedCleanerPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FeedCleanerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for FeedCleanerPlugin {
    fn name(&self) -> &str {
        "FeedCleanerPlugin"
    }

    async fn filter_item(&self, item: &FeedItem) -> anyhow::Result<FilterDecision> {
        let title_empty = item.title.trim().is_empty() || item.title == "Untitled";
        let link_empty = item.link.trim().is_empty();
        let desc_empty = item.description.trim().is_empty();

        // Drop item if it has no title, no link, and no description (completely empty/corrupt entry)
        if title_empty && link_empty && desc_empty {
            return Ok(FilterDecision::Drop);
        }

        // Drop entries that are typical error pages from cloud providers (e.g. Cloudflare / Cloud Functions)
        let lower_title = item.title.to_lowercase();
        if lower_title.contains("403 forbidden")
            || lower_title.contains("502 bad gateway")
            || lower_title.contains("error 404")
        {
            return Ok(FilterDecision::Drop);
        }

        Ok(FilterDecision::Keep)
    }

    async fn transform_item(&self, item: &mut FeedItem) -> anyhow::Result<()> {
        // Clean and trim extra whitespace in titles
        item.title = item.title.split_whitespace().collect::<Vec<_>>().join(" ");
        item.description = item.description.trim().to_string();

        Ok(())
    }
}
