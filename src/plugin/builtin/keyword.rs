use crate::model::FeedItem;
use crate::plugin::{FilterDecision, Plugin};
use async_trait::async_trait;

pub struct KeywordFilterPlugin {
    pub include_keywords: Vec<String>,
    pub exclude_keywords: Vec<String>,
}

impl KeywordFilterPlugin {
    pub fn new(include: Vec<String>, exclude: Vec<String>) -> Self {
        Self {
            include_keywords: include
                .into_iter()
                .map(|k| k.trim().to_lowercase())
                .filter(|k| !k.is_empty())
                .collect(),
            exclude_keywords: exclude
                .into_iter()
                .map(|k| k.trim().to_lowercase())
                .filter(|k| !k.is_empty())
                .collect(),
        }
    }

    fn matches_haystack(&self, haystack: &str, keyword: &str) -> bool {
        haystack.to_lowercase().contains(keyword)
    }

    fn item_text(&self, item: &FeedItem) -> String {
        let mut text = format!("{} {} ", item.title, item.description);
        if let Some(ref c) = item.content {
            text.push_str(c);
            text.push(' ');
        }
        if let Some(ref a) = item.author {
            text.push_str(a);
            text.push(' ');
        }
        for cat in &item.categories {
            text.push_str(cat);
            text.push(' ');
        }
        text
    }
}

#[async_trait]
impl Plugin for KeywordFilterPlugin {
    fn name(&self) -> &str {
        "KeywordFilterPlugin"
    }

    async fn filter_item(&self, item: &FeedItem) -> anyhow::Result<FilterDecision> {
        let full_text = self.item_text(item);

        // 1. Exclude check: If any exclude keyword matches, drop the item
        for exclude in &self.exclude_keywords {
            if self.matches_haystack(&full_text, exclude) {
                return Ok(FilterDecision::Drop);
            }
        }

        // 2. Include check: If include keywords are specified, at least one must match
        if !self.include_keywords.is_empty() {
            let matched = self
                .include_keywords
                .iter()
                .any(|include| self.matches_haystack(&full_text, include));

            if !matched {
                return Ok(FilterDecision::Drop);
            }
        }

        Ok(FilterDecision::Keep)
    }
}
