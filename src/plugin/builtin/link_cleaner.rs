use crate::model::FeedItem;
use crate::plugin::Plugin;
use async_trait::async_trait;
use url::Url;

pub struct LinkCleanerPlugin {
    tracking_params: Vec<&'static str>,
}

impl LinkCleanerPlugin {
    pub fn new() -> Self {
        Self {
            tracking_params: vec![
                "utm_source",
                "utm_medium",
                "utm_campaign",
                "utm_term",
                "utm_content",
                "utm_id",
                "fbclid",
                "gclid",
                "mc_cid",
                "mc_eid",
                "_ga",
                "_hsenc",
                "_hsmi",
            ],
        }
    }

    fn clean_url_string(&self, raw_url: &str) -> Option<String> {
        let mut parsed_url = Url::parse(raw_url).ok()?;
        let filtered_query: Vec<(String, String)> = parsed_url
            .query_pairs()
            .filter(|(key, _)| {
                let k = key.to_lowercase();
                !self.tracking_params.iter().any(|&param| k == param)
            })
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        if filtered_query.is_empty() {
            parsed_url.set_query(None);
        } else {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            for (k, v) in filtered_query {
                serializer.append_pair(&k, &v);
            }
            parsed_url.set_query(Some(&serializer.finish()));
        }

        Some(parsed_url.to_string())
    }
}

impl Default for LinkCleanerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for LinkCleanerPlugin {
    fn name(&self) -> &str {
        "LinkCleanerPlugin"
    }

    async fn transform_item(&self, item: &mut FeedItem) -> anyhow::Result<()> {
        if let Some(cleaned) = self.clean_url_string(&item.link) {
            item.link = cleaned;
        }

        if let Some(cleaned_id) = self.clean_url_string(&item.id) {
            item.id = cleaned_id;
        }

        Ok(())
    }
}
