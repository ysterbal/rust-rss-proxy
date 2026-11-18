#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rss_proxy::config::AppConfig;
    use rss_proxy::fetcher::FeedFetcher;
    use rss_proxy::server::{create_router, AppState};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    fn make_test_state(config: AppConfig, config_path: Option<PathBuf>) -> Arc<AppState> {
        let fetcher = FeedFetcher::new(60);
        Arc::new(AppState {
            config: Arc::new(RwLock::new(config)),
            config_path: config_path.unwrap_or_else(|| PathBuf::from("test-config.yaml")),
            fetcher,
        })
    }

    #[tokio::test]
    async fn test_health_route() {
        let state = make_test_state(AppConfig::default(), None);
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"OK");
    }

    #[tokio::test]
    async fn test_root_status_route() {
        let state = make_test_state(AppConfig::default(), None);
        let app = create_router(state);

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "running");
        assert_eq!(json["auto_register_proxy_feeds"], true);
    }

    #[tokio::test]
    async fn test_plugins_list_route() {
        let config = AppConfig::default();

        let state = make_test_state(config, None);
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/plugins")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let plugins: Vec<String> = serde_json::from_slice(&body).unwrap();
        assert!(plugins.contains(&"filter_sponsors".to_string()));
        assert!(plugins.contains(&"tag_and_prefix".to_string()));
    }

    #[tokio::test]
    async fn test_missing_proxy_url() {
        let state = make_test_state(AppConfig::default(), None);
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/proxy?exclude=crypto")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should fail because `url` param is required
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_opml_export_route() {
        let mut config = AppConfig::default();
        config.feeds.insert(
            "test-feed".to_string(),
            rss_proxy::config::FeedConfig {
                url: "https://example.com/feed.xml".to_string(),
                title_override: Some("My Test Feed".to_string()),
                group: Some("Testing".to_string()),
                nop: false,
                nop_reason: None,
                filters: Default::default(),
            },
        );

        let state = make_test_state(config, None);
        let app = create_router(state);

        let response = app
            .oneshot(Request::builder().uri("/opml").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let xml_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(xml_str.contains("<opml version=\"2.0\">"));
        assert!(xml_str.contains("<outline text=\"Testing\" title=\"Testing\">"));
        assert!(xml_str.contains("xmlUrl=\"http://127.0.0.1:8080/feeds/test-feed\""));
    }

    #[tokio::test]
    async fn test_group_route_not_found() {
        let state = make_test_state(AppConfig::default(), None);
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/groups/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_feed_slug_route_not_found() {
        let state = make_test_state(AppConfig::default(), None);
        let app = create_router(state);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/feeds/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // /feeds/ (and /feeds) should now return the aggregate feed (200 OK), not 404!
        let response_slash = app
            .oneshot(
                Request::builder()
                    .uri("/feeds/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response_slash.status(), StatusCode::OK);
        let body = response_slash
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let xml_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(xml_str.contains("<title>RSS Proxy - All Feeds</title>"));
    }

    #[tokio::test]
    async fn test_nop_feed_configured_route() {
        let mut config = AppConfig::default();
        config.feeds.insert(
            "pwc-latest".to_string(),
            rss_proxy::config::FeedConfig {
                url: "https://us-east1-ml-feeds.cloudfunctions.net/pwc/latest".to_string(),
                title_override: Some("Papers with Code".to_string()),
                group: Some("Tech".to_string()),
                nop: true,
                nop_reason: Some("403 Forbidden - Policy".to_string()),
                filters: Default::default(),
            },
        );

        let state = make_test_state(config, None);
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/feeds/pwc-latest")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let xml_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(xml_str.contains("<title>Papers with Code [NOP]</title>"));
        assert!(xml_str.contains("403 Forbidden"));
    }

    #[tokio::test]
    async fn test_auto_register_proxy_feed_to_config() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_path_buf();

        let config = AppConfig::default();

        let state = make_test_state(config, Some(temp_path.clone()));
        let app = create_router(state.clone());

        // We use a failing URL so it falls back via NopHandlerPlugin and produces a valid feed response
        let test_url = "https://example.com/never-before-seen-feed.xml";
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/proxy?url={}&group=AutoTest&exclude=spam",
                        test_url
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key("X-Feed-Slug"));

        // Check in-memory config
        {
            let cfg = state.config.read().await;
            let registered = cfg.feeds.values().find(|f| f.url == test_url);
            assert!(
                registered.is_some(),
                "Feed should be registered in config.feeds"
            );
            let f = registered.unwrap();
            assert_eq!(f.group.as_deref(), Some("AutoTest"));
            assert_eq!(f.filters.exclude_keywords, vec!["spam".to_string()]);
        }

        // Check persisted file on disk
        let disk_content = std::fs::read_to_string(&temp_path).unwrap();
        assert!(disk_content.contains(test_url));
        assert!(disk_content.contains("AutoTest"));
    }
}
