#[cfg(test)]
mod tests {
    use rss_proxy::opml::*;
    use std::io::Write;

    const SAMPLE_OPML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <head>
    <title>My Subscriptions</title>
  </head>
  <body>
    <outline text="Direct Feed" title="Direct Feed" type="rss" xmlUrl="https://example.com/direct.xml" htmlUrl="https://example.com/"/>
    <outline text="Technology" title="Technology">
      <outline text="Hacker News" title="Hacker News" type="rss" xmlUrl="https://hnrss.org/frontpage" htmlUrl="https://news.ycombinator.com/"/>
      <outline text="Rust Blog" title="Rust Blog" type="rss" xmlUrl="https://blog.rust-lang.org/feed.xml" htmlUrl="https://blog.rust-lang.org/"/>
    </outline>
  </body>
</opml>
"#;

    #[test]
    fn test_parse_opml() {
        let feeds = parse_opml(SAMPLE_OPML).unwrap();
        assert_eq!(feeds.len(), 3);

        assert_eq!(feeds[0].title, "Direct Feed");
        assert_eq!(feeds[0].xml_url, "https://example.com/direct.xml");
        assert_eq!(feeds[0].category, None);

        assert_eq!(feeds[1].title, "Hacker News");
        assert_eq!(feeds[1].xml_url, "https://hnrss.org/frontpage");
        assert_eq!(feeds[1].category, Some("Technology".to_string()));

        assert_eq!(feeds[2].title, "Rust Blog");
        assert_eq!(feeds[2].xml_url, "https://blog.rust-lang.org/feed.xml");
        assert_eq!(feeds[2].category, Some("Technology".to_string()));
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hacker News!"), "hacker-news");
        assert_eq!(
            slugify("Rust 1.94 Release & More"),
            "rust-1-94-release-more"
        );
        assert_eq!(slugify("   ___   "), "feed");
    }

    #[test]
    fn test_unique_slug() {
        let mut existing = std::collections::HashSet::new();
        existing.insert("tech-news".to_string());

        assert_eq!(unique_slug("tech-news", &existing), "tech-news-2");

        existing.insert("tech-news-2".to_string());
        assert_eq!(unique_slug("tech-news", &existing), "tech-news-3");
    }

    #[test]
    fn test_import_opml_into_config() {
        let mut opml_file = tempfile::NamedTempFile::new().unwrap();
        write!(opml_file, "{}", SAMPLE_OPML).unwrap();

        let config_file = tempfile::NamedTempFile::new().unwrap();

        // Dry run test
        let dry_imported =
            import_opml_into_config(opml_file.path(), config_file.path(), true).unwrap();
        assert_eq!(dry_imported.len(), 3);
        let dry_config_content = std::fs::read_to_string(config_file.path()).unwrap();
        assert!(dry_config_content.is_empty());

        // Actual import
        let imported =
            import_opml_into_config(opml_file.path(), config_file.path(), false).unwrap();
        assert_eq!(imported.len(), 3);

        let config = rss_proxy::config::AppConfig::load_or_default(config_file.path()).unwrap();
        assert_eq!(config.feeds.len(), 3);
        assert!(config.feeds.contains_key("hacker-news"));
        assert!(config.feeds.contains_key("rust-blog"));
        assert!(config.feeds.contains_key("direct-feed"));
    }

    #[test]
    fn test_generate_opml() {
        let feeds = vec![
            ExportableFeed {
                title: "Calculated Risk".to_string(),
                xml_url: "http://127.0.0.1:8080/feeds/calculated-risk".to_string(),
                html_url: Some("http://www.calculatedriskblog.com/".to_string()),
                group: Some("Business".to_string()),
            },
            ExportableFeed {
                title: "Rust & Friends".to_string(),
                xml_url: "http://127.0.0.1:8080/feeds/rust-friends".to_string(),
                html_url: None,
                group: None,
            },
        ];

        let opml = generate_grouped_opml("Proxied Feeds", &feeds);
        assert!(opml.contains("<opml version=\"2.0\">"));
        assert!(opml.contains("<outline text=\"Business\" title=\"Business\">"));
        assert!(opml.contains("xmlUrl=\"http://127.0.0.1:8080/feeds/calculated-risk\""));
        assert!(opml.contains("title=\"Rust &amp; Friends\""));
    }
}
