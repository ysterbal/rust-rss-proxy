use crate::config::{AppConfig, FeedConfig};
use crate::fetcher::FeedFetcher;
use crate::opml::{generate_grouped_opml, ExportableFeed};
use crate::plugin::builtin::{
    AgeFilterPlugin, KeywordFilterPlugin, LimitPlugin, LinkCleanerPlugin, RegexFilterPlugin,
};
use crate::plugin::rhai_plugin::RhaiPlugin;
use crate::plugin::{Plugin, PluginPipeline};
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::error;

pub struct AppState {
    pub config: AppConfig,
    pub fetcher: FeedFetcher,
}

#[derive(Debug, Deserialize)]
pub struct ProxyQuery {
    pub url: String,
    pub include: Option<String>,
    pub exclude: Option<String>,
    pub include_regex: Option<String>,
    pub exclude_regex: Option<String>,
    pub clean_links: Option<bool>,
    pub max_age_days: Option<i64>,
    pub limit: Option<usize>,
    pub script: Option<String>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(handle_root))
        .route("/health", get(handle_health))
        .route("/plugins", get(handle_list_plugins))
        .route("/opml", get(handle_export_opml))
        .route("/feeds", get(handle_all_feeds))
        .route("/feeds/", get(handle_all_feeds))
        .route("/feeds/{slug}", get(handle_configured_feed))
        .route("/feeds/{slug}/", get(handle_configured_feed))
        .route("/groups/{group}", get(handle_group_feed))
        .route("/groups/{group}/", get(handle_group_feed))
        .route("/categories/{group}", get(handle_group_feed))
        .route("/categories/{group}/", get(handle_group_feed))
        .route("/proxy", get(handle_proxy_feed))
        .with_state(state)
}

#[derive(Serialize)]
struct StatusResponse {
    status: &'static str,
    version: &'static str,
    configured_feeds: Vec<String>,
    groups: Vec<String>,
    available_plugins: Vec<String>,
}

async fn handle_root(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    let configured_feeds = state.config.feeds.keys().cloned().collect();
    let mut groups_set = HashSet::new();
    for f in state.config.feeds.values() {
        if let Some(ref g) = f.group {
            groups_set.insert(g.clone());
        }
    }
    let mut groups: Vec<String> = groups_set.into_iter().collect();
    groups.sort();

    let plugins = RhaiPlugin::load_directory(&state.config.plugins_dir);
    let available_plugins = plugins.iter().map(|p| p.name().to_string()).collect();

    Json(StatusResponse {
        status: "running",
        version: env!("CARGO_PKG_VERSION"),
        configured_feeds,
        groups,
        available_plugins,
    })
}

async fn handle_health() -> &'static str {
    "OK"
}

async fn handle_list_plugins(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    let plugins = RhaiPlugin::load_directory(&state.config.plugins_dir);
    Json(plugins.into_iter().map(|p| p.name().to_string()).collect())
}

async fn handle_export_opml(State(state): State<Arc<AppState>>) -> Response {
    let base_url = format!(
        "http://{}:{}",
        state.config.server.host, state.config.server.port
    );

    let feeds: Vec<ExportableFeed> = state
        .config
        .feeds
        .iter()
        .map(|(slug, cfg)| {
            let title = cfg
                .title_override
                .clone()
                .unwrap_or_else(|| slug.clone());
            let proxy_url = format!("{}/feeds/{}", base_url, slug);
            ExportableFeed {
                title,
                xml_url: proxy_url,
                html_url: None,
                group: cfg.group.clone(),
            }
        })
        .collect();

    let opml_xml = generate_grouped_opml("RSS Proxy Filtered Subscriptions", &feeds);

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/x-opml; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"rss-proxy-subscriptions.opml\"",
            ),
        ],
        opml_xml,
    )
        .into_response()
}

fn build_pipeline_for_feed(feed_cfg: &FeedConfig, plugins_dir: &str) -> Result<PluginPipeline, String> {
    let mut pipeline = PluginPipeline::new();

    // 1. Built-in keyword filter
    if !feed_cfg.filters.include_keywords.is_empty() || !feed_cfg.filters.exclude_keywords.is_empty() {
        pipeline.add_plugin(Box::new(KeywordFilterPlugin::new(
            feed_cfg.filters.include_keywords.clone(),
            feed_cfg.filters.exclude_keywords.clone(),
        )));
    }

    // 2. Built-in regex filter
    if !feed_cfg.filters.include_regex.is_empty() || !feed_cfg.filters.exclude_regex.is_empty() {
        match RegexFilterPlugin::new(
            &feed_cfg.filters.include_regex,
            &feed_cfg.filters.exclude_regex,
        ) {
            Ok(plugin) => pipeline.add_plugin(Box::new(plugin)),
            Err(e) => return Err(format!("Invalid regex configuration: {}", e)),
        }
    }

    // 3. Built-in link cleaner
    if feed_cfg.filters.clean_links {
        pipeline.add_plugin(Box::new(LinkCleanerPlugin::new()));
    }

    // 4. Built-in max age filter
    if let Some(days) = feed_cfg.filters.max_age_days {
        pipeline.add_plugin(Box::new(AgeFilterPlugin::from_days(days)));
    }

    // 5. Rhai script plugins
    for script_name in &feed_cfg.filters.scripts {
        let script_file = if script_name.ends_with(".rhai") {
            script_name.clone()
        } else {
            format!("{}.rhai", script_name)
        };
        let path = PathBuf::from(plugins_dir).join(&script_file);
        match RhaiPlugin::from_file(&path) {
            Ok(plugin) => pipeline.add_plugin(Box::new(plugin)),
            Err(e) => {
                error!("Failed to load plugin script {}: {}", path.display(), e);
            }
        }
    }

    // 6. Built-in limit plugin
    if let Some(limit) = feed_cfg.filters.max_items {
        pipeline.add_plugin(Box::new(LimitPlugin::new(limit)));
    }

    Ok(pipeline)
}

/// Serves an aggregated RSS feed containing posts from ALL configured feeds
async fn handle_all_feeds(State(state): State<Arc<AppState>>) -> Response {
    render_aggregated_feed(
        &state,
        "RSS Proxy - All Feeds",
        &state.config.feeds.iter().collect::<Vec<_>>(),
    )
    .await
}

/// Serves an aggregated RSS feed for a specific group/category of feeds
async fn handle_group_feed(
    Path(group_param): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let clean_group = group_param.trim_end_matches('/').to_lowercase();
    let matching_feeds: Vec<(&String, &FeedConfig)> = state
        .config
        .feeds
        .iter()
        .filter(|(_, cfg)| {
            cfg.group
                .as_ref()
                .map(|g| g.to_lowercase() == clean_group)
                .unwrap_or(false)
        })
        .collect();

    if matching_feeds.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            format!("No feeds found in group '{}'", group_param),
        )
            .into_response();
    }

    let group_title = matching_feeds
        .first()
        .and_then(|(_, cfg)| cfg.group.clone())
        .unwrap_or(group_param);

    render_aggregated_feed(
        &state,
        &format!("RSS Proxy - {}", group_title),
        &matching_feeds,
    )
    .await
}

async fn render_aggregated_feed(
    state: &Arc<AppState>,
    feed_title: &str,
    feeds_to_include: &[(&String, &FeedConfig)],
) -> Response {
    if feeds_to_include.is_empty() {
        let empty_feed = crate::model::Feed {
            title: feed_title.to_string(),
            link: format!("http://{}:{}/feeds", state.config.server.host, state.config.server.port),
            description: "No feeds in this view".to_string(),
            language: Some("en".to_string()),
            items: Vec::new(),
        };
        return match empty_feed.to_rss_xml() {
            Ok(xml) => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
                xml,
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        };
    }

    let mut all_items = Vec::new();

    for (slug, feed_cfg) in feeds_to_include {
        let mut feed = match state.fetcher.fetch(&feed_cfg.url).await {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to fetch upstream feed for {}: {}", slug, e);
                continue;
            }
        };

        if let Some(ref override_title) = feed_cfg.title_override {
            feed.title = override_title.clone();
        }

        let pipeline = match build_pipeline_for_feed(feed_cfg, &state.config.plugins_dir) {
            Ok(p) => p,
            Err(e) => {
                error!("Error building pipeline for {}: {}", slug, e);
                continue;
            }
        };

        if let Err(e) = pipeline.process(&mut feed).await {
            error!("Error processing pipeline for {}: {}", slug, e);
            continue;
        }

        all_items.extend(feed.items);
    }

    // Sort by publication date, newest first
    all_items.sort_by(|a, b| {
        let date_a = a.published.or(a.updated);
        let date_b = b.published.or(b.updated);
        date_b.cmp(&date_a)
    });

    // Deduplicate items by ID or link
    let mut seen_ids = HashSet::new();
    all_items.retain(|item| {
        let key = if !item.id.is_empty() {
            item.id.clone()
        } else {
            item.link.clone()
        };
        seen_ids.insert(key)
    });

    if all_items.len() > 100 {
        all_items.truncate(100);
    }

    let aggregate_feed = crate::model::Feed {
        title: feed_title.to_string(),
        link: format!("http://{}:{}/feeds", state.config.server.host, state.config.server.port),
        description: format!("Aggregated and filtered feeds for {}", feed_title),
        language: Some("en".to_string()),
        items: all_items,
    };

    match aggregate_feed.to_rss_xml() {
        Ok(xml) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
            xml,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("XML generation error: {}", e),
        )
            .into_response(),
    }
}

async fn handle_configured_feed(
    Path(slug): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let clean_slug = slug.trim_end_matches('/');
    let feed_cfg = match state.config.feeds.get(clean_slug) {
        Some(cfg) => cfg,
        None => {
            return (
                StatusCode::NOT_FOUND,
                format!("Feed with slug '{}' not found in config", clean_slug),
            )
                .into_response();
        }
    };

    // 1. Build pipeline
    let pipeline = match build_pipeline_for_feed(feed_cfg, &state.config.plugins_dir) {
        Ok(p) => p,
        Err(e) => {
            error!("Invalid config for feed {}: {}", clean_slug, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
        }
    };

    // 2. Fetch feed
    let mut feed = match state.fetcher.fetch(&feed_cfg.url).await {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to fetch upstream feed {}: {}", feed_cfg.url, e);
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to fetch upstream feed: {}", e),
            )
                .into_response();
        }
    };

    if let Some(ref override_title) = feed_cfg.title_override {
        feed.title = override_title.clone();
    }

    // 3. Run through pipeline
    if let Err(e) = pipeline.process(&mut feed).await {
        error!("Pipeline processing failed for {}: {}", clean_slug, e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Pipeline error: {}", e),
        )
            .into_response();
    }

    // 4. Render XML
    match feed.to_rss_xml() {
        Ok(xml) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
            xml,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("XML generation error: {}", e),
        )
            .into_response(),
    }
}

async fn handle_proxy_feed(
    Query(params): Query<ProxyQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if params.url.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing required 'url' query parameter").into_response();
    }

    let mut pipeline = PluginPipeline::new();

    // Keywords
    let include_kws: Vec<String> = params
        .include
        .map(|s| s.split(',').map(|k| k.trim().to_string()).collect())
        .unwrap_or_default();
    let exclude_kws: Vec<String> = params
        .exclude
        .map(|s| s.split(',').map(|k| k.trim().to_string()).collect())
        .unwrap_or_default();

    if !include_kws.is_empty() || !exclude_kws.is_empty() {
        pipeline.add_plugin(Box::new(KeywordFilterPlugin::new(include_kws, exclude_kws)));
    }

    // Regex
    let include_regex = params
        .include_regex
        .map(|r| vec![r])
        .unwrap_or_default();
    let exclude_regex = params
        .exclude_regex
        .map(|r| vec![r])
        .unwrap_or_default();

    if !include_regex.is_empty() || !exclude_regex.is_empty() {
        match RegexFilterPlugin::new(&include_regex, &exclude_regex) {
            Ok(p) => pipeline.add_plugin(Box::new(p)),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid regex parameter: {}", e),
                )
                    .into_response();
            }
        }
    }

    // Link cleaner (default true)
    if params.clean_links.unwrap_or(true) {
        pipeline.add_plugin(Box::new(LinkCleanerPlugin::new()));
    }

    // Max age
    if let Some(days) = params.max_age_days {
        pipeline.add_plugin(Box::new(AgeFilterPlugin::from_days(days)));
    }

    // Custom Rhai script
    if let Some(script_name) = params.script {
        let script_file = if script_name.ends_with(".rhai") {
            script_name
        } else {
            format!("{}.rhai", script_name)
        };
        let path = PathBuf::from(&state.config.plugins_dir).join(&script_file);
        match RhaiPlugin::from_file(&path) {
            Ok(p) => pipeline.add_plugin(Box::new(p)),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    format!("Failed to load plugin '{}': {}", script_file, e),
                )
                    .into_response();
            }
        }
    }

    // Limit
    if let Some(limit) = params.limit {
        pipeline.add_plugin(Box::new(LimitPlugin::new(limit)));
    }

    // Fetch feed
    let mut feed = match state.fetcher.fetch(&params.url).await {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to fetch upstream feed: {}", e),
            )
                .into_response();
        }
    };

    // Filter feed
    if let Err(e) = pipeline.process(&mut feed).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Pipeline error: {}", e),
        )
            .into_response();
    }

    // Render XML
    match feed.to_rss_xml() {
        Ok(xml) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
            xml,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("XML generation error: {}", e),
        )
            .into_response(),
    }
}
