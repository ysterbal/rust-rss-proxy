use crate::model::Feed;
use crate::plugin::Plugin;
use async_trait::async_trait;

pub struct LimitPlugin {
    pub max_items: usize,
}

impl LimitPlugin {
    pub fn new(max_items: usize) -> Self {
        Self { max_items }
    }
}

#[async_trait]
impl Plugin for LimitPlugin {
    fn name(&self) -> &str {
        "LimitPlugin"
    }

    async fn post_process_feed(&self, feed: &mut Feed) -> anyhow::Result<()> {
        if feed.items.len() > self.max_items {
            feed.items.truncate(self.max_items);
        }
        Ok(())
    }
}
