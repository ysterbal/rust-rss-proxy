#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rss_proxy::config::AppConfig;
    use rss_proxy::fetcher::FeedFetcher;
    use rss_proxy::server::{create_router, AppState};
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_route() {
        let config = AppConfig::default();
        let fetcher = FeedFetcher::new(60);
        let state = Arc::new(AppState { config, fetcher });
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
        let config = AppConfig::default();
        let fetcher = FeedFetcher::new(60);
        let state = Arc::new(AppState { config, fetcher });
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "running");
    }

    #[tokio::test]
    async fn test_plugins_list_route() {
        let mut config = AppConfig::default();
        config.plugins_dir = "./plugins".to_string();

        let fetcher = FeedFetcher::new(60);
        let state = Arc::new(AppState { config, fetcher });
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
        let config = AppConfig::default();
        let fetcher = FeedFetcher::new(60);
        let state = Arc::new(AppState { config, fetcher });
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
                filters: Default::default(),
            },
        );

        let fetcher = FeedFetcher::new(60);
        let state = Arc::new(AppState { config, fetcher });
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/opml")
                    .body(Body::empty())
                    .unwrap(),
            )
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
        let config = AppConfig::default();
        let fetcher = FeedFetcher::new(60);
        let state = Arc::new(AppState { config, fetcher });
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
        let config = AppConfig::default();
        let fetcher = FeedFetcher::new(60);
        let state = Arc::new(AppState { config, fetcher });
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
        let body = response_slash.into_body().collect().await.unwrap().to_bytes();
        let xml_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(xml_str.contains("<title>RSS Proxy - All Feeds</title>"));
    }
}
