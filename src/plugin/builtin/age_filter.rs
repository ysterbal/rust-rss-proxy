use crate::model::FeedItem;
use crate::plugin::{FilterDecision, Plugin};
use async_trait::async_trait;
use chrono::{Duration, Utc};

pub struct AgeFilterPlugin {
    pub max_age: Duration,
}

impl AgeFilterPlugin {
    pub fn from_days(days: i64) -> Self {
        Self {
            max_age: Duration::days(days),
        }
    }

    #[allow(dead_code)]
    pub fn from_hours(hours: i64) -> Self {
        Self {
            max_age: Duration::hours(hours),
        }
    }
}

#[async_trait]
impl Plugin for AgeFilterPlugin {
    fn name(&self) -> &str {
        "AgeFilterPlugin"
    }

    async fn filter_item(&self, item: &FeedItem) -> anyhow::Result<FilterDecision> {
        let now = Utc::now();
        if let Some(date) = item.published.or(item.updated) {
            if now - date > self.max_age {
                return Ok(FilterDecision::Drop);
            }
        }
        Ok(FilterDecision::Keep)
    }
}
