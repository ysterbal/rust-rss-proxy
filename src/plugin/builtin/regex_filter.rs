use crate::model::FeedItem;
use crate::plugin::{FilterDecision, Plugin};
use async_trait::async_trait;
use regex::Regex;

pub struct RegexFilterPlugin {
    pub include_patterns: Vec<Regex>,
    pub exclude_patterns: Vec<Regex>,
}

impl RegexFilterPlugin {
    pub fn new(include: &[String], exclude: &[String]) -> anyhow::Result<Self> {
        let include_patterns = include
            .iter()
            .map(|p| Regex::new(p))
            .collect::<Result<Vec<_>, _>>()?;

        let exclude_patterns = exclude
            .iter()
            .map(|p| Regex::new(p))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            include_patterns,
            exclude_patterns,
        })
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
impl Plugin for RegexFilterPlugin {
    fn name(&self) -> &str {
        "RegexFilterPlugin"
    }

    async fn filter_item(&self, item: &FeedItem) -> anyhow::Result<FilterDecision> {
        let full_text = self.item_text(item);

        for re in &self.exclude_patterns {
            if re.is_match(&full_text) {
                return Ok(FilterDecision::Drop);
            }
        }

        if !self.include_patterns.is_empty() {
            let matched = self
                .include_patterns
                .iter()
                .any(|re| re.is_match(&full_text));
            if !matched {
                return Ok(FilterDecision::Drop);
            }
        }

        Ok(FilterDecision::Keep)
    }
}
