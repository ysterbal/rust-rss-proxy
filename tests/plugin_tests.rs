#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use rss_proxy::model::{Feed, FeedItem};
    use rss_proxy::plugin::builtin::*;
    use rss_proxy::plugin::rhai_plugin::RhaiPlugin;
    use rss_proxy::plugin::{FilterDecision, Plugin, PluginPipeline};

    fn sample_item(title: &str, link: &str, desc: &str) -> FeedItem {
        FeedItem {
            id: link.to_string(),
            title: title.to_string(),
            link: link.to_string(),
            description: desc.to_string(),
            content: None,
            author: Some("Tester".to_string()),
            published: Some(Utc::now()),
            updated: None,
            categories: vec!["tech".to_string()],
        }
    }

    #[tokio::test]
    async fn test_keyword_filter_exclude() {
        let filter =
            KeywordFilterPlugin::new(vec![], vec!["sponsored".to_string(), "crypto".to_string()]);

        let item1 = sample_item(
            "Great Rust tutorial",
            "https://example.com/1",
            "A normal post",
        );
        let item2 = sample_item("Sponsored: buy this!", "https://example.com/2", "Spam");

        assert_eq!(
            filter.filter_item(&item1).await.unwrap(),
            FilterDecision::Keep
        );
        assert_eq!(
            filter.filter_item(&item2).await.unwrap(),
            FilterDecision::Drop
        );
    }

    #[tokio::test]
    async fn test_keyword_filter_include() {
        let filter =
            KeywordFilterPlugin::new(vec!["rust".to_string(), "tokio".to_string()], vec![]);

        let item1 = sample_item("Great Rust tutorial", "https://example.com/1", "Learn Rust");
        let item2 = sample_item("Daily JS news", "https://example.com/2", "JavaScript tips");

        assert_eq!(
            filter.filter_item(&item1).await.unwrap(),
            FilterDecision::Keep
        );
        assert_eq!(
            filter.filter_item(&item2).await.unwrap(),
            FilterDecision::Drop
        );
    }

    #[tokio::test]
    async fn test_regex_filter() {
        let filter = RegexFilterPlugin::new(
            &["(?i)rust.*release".to_string()],
            &["(?i)beta".to_string()],
        )
        .unwrap();

        let item1 = sample_item(
            "Rust 1.94 Release is out",
            "https://example.com/1",
            "New release",
        );
        let item2 = sample_item(
            "Rust 1.95 beta Release",
            "https://example.com/2",
            "Beta preview",
        );
        let item3 = sample_item(
            "Random announcement",
            "https://example.com/3",
            "Other stuff",
        );

        assert_eq!(
            filter.filter_item(&item1).await.unwrap(),
            FilterDecision::Keep
        );
        assert_eq!(
            filter.filter_item(&item2).await.unwrap(),
            FilterDecision::Drop
        );
        assert_eq!(
            filter.filter_item(&item3).await.unwrap(),
            FilterDecision::Drop
        );
    }

    #[tokio::test]
    async fn test_link_cleaner() {
        let cleaner = LinkCleanerPlugin::new();
        let mut item = sample_item(
            "Article with tracking",
            "https://example.com/post?id=123&utm_source=twitter&utm_medium=social&utm_campaign=winter#heading",
            "Some text",
        );

        cleaner.transform_item(&mut item).await.unwrap();

        assert_eq!(item.link, "https://example.com/post?id=123#heading");
    }

    #[tokio::test]
    async fn test_age_filter() {
        let filter = AgeFilterPlugin::from_days(7);

        let mut old_item = sample_item("Old news", "https://example.com/old", "Old post");
        old_item.published = Some(Utc::now() - Duration::days(10));

        let fresh_item = sample_item("Fresh news", "https://example.com/fresh", "Fresh post");

        assert_eq!(
            filter.filter_item(&old_item).await.unwrap(),
            FilterDecision::Drop
        );
        assert_eq!(
            filter.filter_item(&fresh_item).await.unwrap(),
            FilterDecision::Keep
        );
    }

    #[tokio::test]
    async fn test_limit_plugin() {
        let limit = LimitPlugin::new(2);
        let mut feed = Feed {
            title: "Test Feed".to_string(),
            link: "https://example.com".to_string(),
            description: "Test".to_string(),
            language: None,
            items: vec![
                sample_item("1", "https://example.com/1", ""),
                sample_item("2", "https://example.com/2", ""),
                sample_item("3", "https://example.com/3", ""),
            ],
        };

        limit.post_process_feed(&mut feed).await.unwrap();
        assert_eq!(feed.items.len(), 2);
        assert_eq!(feed.items[0].title, "1");
        assert_eq!(feed.items[1].title, "2");
    }

    #[tokio::test]
    async fn test_rhai_script_plugin() {
        use std::io::Write;
        let script_code = r#"
            fn filter_item(item) {
                if item.title.to_lower().contains("crypto") {
                    return "drop";
                }
                return "keep";
            }

            fn transform_item(item) {
                item.title = "[Clean] " + item.title;
                return item;
            }
        "#;

        let mut temp_file = tempfile::NamedTempFile::new().unwrap();
        write!(temp_file, "{}", script_code).unwrap();

        let plugin = RhaiPlugin::from_file(temp_file.path()).unwrap();

        let crypto_item = sample_item("Crypto surges today", "https://example.com/c", "");
        let tech_item = sample_item("New compiler optimizations", "https://example.com/t", "");

        assert_eq!(
            plugin.filter_item(&crypto_item).await.unwrap(),
            FilterDecision::Drop
        );
        assert_eq!(
            plugin.filter_item(&tech_item).await.unwrap(),
            FilterDecision::Keep
        );

        let mut mod_item = tech_item;
        plugin.transform_item(&mut mod_item).await.unwrap();
        assert_eq!(mod_item.title, "[Clean] New compiler optimizations");
    }

    #[tokio::test]
    async fn test_full_pipeline_processing() {
        let mut pipeline = PluginPipeline::new();

        // 1. Keyword filter: exclude "spam"
        pipeline.add_plugin(Box::new(KeywordFilterPlugin::new(
            vec![],
            vec!["spam".to_string()],
        )));

        // 2. Link cleaner
        pipeline.add_plugin(Box::new(LinkCleanerPlugin::new()));

        // 3. Limit plugin: 2 items
        pipeline.add_plugin(Box::new(LimitPlugin::new(2)));

        let mut feed = Feed {
            title: "Pipeline Feed".to_string(),
            link: "https://example.com".to_string(),
            description: "Testing pipeline".to_string(),
            language: Some("en".to_string()),
            items: vec![
                sample_item(
                    "First item",
                    "https://example.com/1?utm_source=rss",
                    "Clean",
                ),
                sample_item("Spam post", "https://example.com/spam", "Buy now"),
                sample_item(
                    "Second item",
                    "https://example.com/2?utm_medium=feed",
                    "Also clean",
                ),
                sample_item("Third item", "https://example.com/3", "Clean too"),
            ],
        };

        pipeline.process(&mut feed).await.unwrap();

        // Spam item dropped, 3 items remain, limit truncates to 2 items
        assert_eq!(feed.items.len(), 2);
        assert_eq!(feed.items[0].title, "First item");
        assert_eq!(feed.items[0].link, "https://example.com/1");
        assert_eq!(feed.items[1].title, "Second item");
        assert_eq!(feed.items[1].link, "https://example.com/2");

        // Verify XML serialization works on the filtered feed
        let xml = feed.to_rss_xml().unwrap();
        assert!(xml.contains("<title>First item</title>"));
        assert!(!xml.contains("Spam post"));
        assert!(!xml.contains("utm_source"));
    }

    #[tokio::test]
    async fn test_nop_handler_plugin() {
        let plugin = NopHandlerPlugin::new();
        let err = anyhow::anyhow!("403 Forbidden - Not allowed by policy");

        let nop_feed = plugin
            .on_fetch_error("https://example.com/bad", "Broken Feed", &err)
            .await
            .expect("Should produce a NOP feed");

        assert_eq!(nop_feed.title, "Broken Feed [NOP]");
        assert!(nop_feed.description.contains("403 Forbidden"));
        assert_eq!(nop_feed.items.len(), 0);

        let xml = nop_feed.to_rss_xml().unwrap();
        assert!(xml.contains("<title>Broken Feed [NOP]</title>"));
    }

    #[tokio::test]
    async fn test_feed_cleaner_plugin() {
        let cleaner = FeedCleanerPlugin::new();

        // 1. Corrupt empty item should be dropped
        let empty_item = FeedItem {
            id: "".to_string(),
            title: "".to_string(),
            link: "".to_string(),
            description: "".to_string(),
            content: None,
            author: None,
            published: None,
            updated: None,
            categories: vec![],
        };
        assert_eq!(
            cleaner.filter_item(&empty_item).await.unwrap(),
            FilterDecision::Drop
        );

        // 2. Normal item with messy whitespace should be cleaned
        let mut messy_item = sample_item(
            "  Multi-space   title  here ",
            "https://example.com/ok",
            "  some desc  ",
        );
        assert_eq!(
            cleaner.filter_item(&messy_item).await.unwrap(),
            FilterDecision::Keep
        );
        cleaner.transform_item(&mut messy_item).await.unwrap();
        assert_eq!(messy_item.title, "Multi-space title here");
        assert_eq!(messy_item.description, "some desc");
    }
}
