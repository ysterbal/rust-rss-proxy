use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedItem {
    pub id: String,
    pub title: String,
    pub link: String,
    pub description: String,
    pub content: Option<String>,
    pub author: Option<String>,
    pub published: Option<DateTime<Utc>>,
    pub updated: Option<DateTime<Utc>>,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feed {
    pub title: String,
    pub link: String,
    pub description: String,
    pub language: Option<String>,
    pub items: Vec<FeedItem>,
}

impl Feed {
    /// Convert parsed `feed_rs::model::Feed` to our clean internal `Feed` representation
    pub fn from_feed_rs(parsed: feed_rs::model::Feed) -> Self {
        let title = parsed
            .title
            .map(|t| t.content)
            .unwrap_or_else(|| "Untitled Feed".to_string());

        let link = parsed
            .links
            .first()
            .map(|l| l.href.clone())
            .unwrap_or_default();

        let description = parsed
            .description
            .map(|d| d.content)
            .unwrap_or_else(|| title.clone());

        let language = parsed.language;

        let items = parsed
            .entries
            .into_iter()
            .map(|entry| {
                let id = entry.id;
                let title = entry
                    .title
                    .map(|t| t.content)
                    .unwrap_or_else(|| "Untitled".to_string());

                let link = entry
                    .links
                    .first()
                    .map(|l| l.href.clone())
                    .unwrap_or_default();

                let description = entry
                    .summary
                    .map(|s| s.content)
                    .unwrap_or_else(|| {
                        entry
                            .content
                            .as_ref()
                            .and_then(|c| c.body.clone())
                            .unwrap_or_default()
                    });

                let content = entry.content.and_then(|c| c.body);

                let author = entry
                    .authors
                    .first()
                    .map(|a| a.name.clone());

                let published = entry.published;
                let updated = entry.updated;

                let categories = entry
                    .categories
                    .into_iter()
                    .map(|c| c.term)
                    .collect();

                FeedItem {
                    id,
                    title,
                    link,
                    description,
                    content,
                    author,
                    published,
                    updated,
                    categories,
                }
            })
            .collect();

        Self {
            title,
            link,
            description,
            language,
            items,
        }
    }

    /// Render internal Feed representation to RSS 2.0 XML
    pub fn to_rss_xml(&self) -> Result<String, String> {
        use rss::{ChannelBuilder, ItemBuilder, GuidBuilder};

        let mut channel_builder = ChannelBuilder::default();
        channel_builder
            .title(self.title.clone())
            .link(self.link.clone())
            .description(self.description.clone());

        if let Some(ref lang) = self.language {
            channel_builder.language(Some(lang.clone()));
        }

        let rss_items: Vec<rss::Item> = self
            .items
            .iter()
            .map(|item| {
                let mut ib = ItemBuilder::default();
                ib.title(Some(item.title.clone()))
                    .link(Some(item.link.clone()))
                    .description(Some(item.description.clone()));

                let mut guid_builder = GuidBuilder::default();
                guid_builder.value(if item.id.is_empty() {
                    item.link.clone()
                } else {
                    item.id.clone()
                });
                guid_builder.permalink(item.id.starts_with("http"));
                ib.guid(Some(guid_builder.build()));

                if let Some(pub_date) = item.published.or(item.updated) {
                    ib.pub_date(Some(pub_date.to_rfc2822()));
                }

                if let Some(ref author) = item.author {
                    ib.author(Some(author.clone()));
                }

                let categories: Vec<rss::Category> = item
                    .categories
                    .iter()
                    .map(|c| {
                        let mut cat = rss::Category::default();
                        cat.set_name(c.clone());
                        cat
                    })
                    .collect();
                ib.categories(categories);

                ib.build()
            })
            .collect();

        channel_builder.items(rss_items);
        let channel = channel_builder.build();
        Ok(channel.to_string())
    }
}
