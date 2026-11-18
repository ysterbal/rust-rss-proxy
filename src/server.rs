use crate::config::{AppConfig, FeedConfig, FeedFilterConfig};
use crate::fetcher::FeedFetcher;
use crate::opml::{generate_grouped_opml, ExportableFeed};
use crate::plugin::builtin::{
    AgeFilterPlugin, FeedCleanerPlugin, KeywordFilterPlugin, LimitPlugin, LinkCleanerPlugin,
    NopHandlerPlugin, RegexFilterPlugin,
};
use crate::plugin::rhai_plugin::RhaiPlugin;
use crate::plugin::{Plugin, PluginPipeline};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    pub config_path: PathBuf,
    pub fetcher: FeedFetcher,
}

#[derive(Debug, Deserialize)]
pub struct ProxyQuery {
    pub url: String,
    pub group: Option<String>,
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
    auto_register_proxy_feeds: bool,
}

async fn handle_root(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    let (configured_feeds, groups, plugins_dir, auto_register) = {
        let cfg = state.config.read().await;
        let feeds = cfg.feeds.keys().cloned().collect();
        let mut groups_set = HashSet::new();
        for f in cfg.feeds.values() {
            if let Some(ref g) = f.group {
                groups_set.insert(g.clone());
            }
        }
        let mut groups: Vec<String> = groups_set.into_iter().collect();
        groups.sort();
        (
            feeds,
            groups,
            cfg.plugins_dir.clone(),
            cfg.auto_register_proxy_feeds,
        )
    };

    let plugins = RhaiPlugin::load_directory(&plugins_dir);
    let available_plugins = plugins.iter().map(|p| p.name().to_string()).collect();

    Json(StatusResponse {
        status: "running",
        version: env!("CARGO_PKG_VERSION"),
        configured_feeds,
        groups,
        available_plugins,
        auto_register_proxy_feeds: auto_register,
    })
}

async fn handle_health() -> &'static str {
    "OK"
}

async fn handle_list_plugins(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    let plugins_dir = {
        let cfg = state.config.read().await;
        cfg.plugins_dir.clone()
    };
    let plugins = RhaiPlugin::load_directory(&plugins_dir);
    Json(plugins.into_iter().map(|p| p.name().to_string()).collect())
}

async fn handle_export_opml(State(state): State<Arc<AppState>>) -> Response {
    let feeds = {
        let cfg = state.config.read().await;
        let base = format!("http://{}:{}", cfg.server.host, cfg.server.port);
        let list: Vec<ExportableFeed> = cfg
            .feeds
            .iter()
            .map(|(slug, f)| {
                let title = f.title_override.clone().unwrap_or_else(|| slug.clone());
                let proxy_url = format!("{}/feeds/{}", base, slug);
                ExportableFeed {
                    title,
                    xml_url: proxy_url,
                    html_url: None,
                    group: f.group.clone(),
                }
            })
            .collect();
        list
    };

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

fn build_pipeline_for_feed(
    feed_cfg: &FeedConfig,
    plugins_dir: &str,
) -> Result<PluginPipeline, String> {
    let mut pipeline = PluginPipeline::new();

    // 0. Feed cleaner (sanitizes broken or empty items)
    pipeline.add_plugin(Box::new(FeedCleanerPlugin::new()));

    // 0.1 NOP handler fallback (catches fetch/parse errors and serves valid NOP feed)
    pipeline.add_plugin(Box::new(NopHandlerPlugin::new()));

    // 1. Built-in keyword filter
    if !feed_cfg.filters.include_keywords.is_empty()
        || !feed_cfg.filters.exclude_keywords.is_empty()
    {
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
    let (feeds_list, host, port, plugins_dir) = {
        let cfg = state.config.read().await;
        let list: Vec<(String, FeedConfig)> = cfg
            .feeds
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        (
            list,
            cfg.server.host.clone(),
            cfg.server.port,
            cfg.plugins_dir.clone(),
        )
    };

    render_aggregated_feed(
        &state,
        "RSS Proxy - All Feeds",
        &feeds_list,
        &host,
        port,
        &plugins_dir,
    )
    .await
}

/// Serves an aggregated RSS feed for a specific group/category of feeds
async fn handle_group_feed(
    Path(group_param): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let clean_group = group_param.trim_end_matches('/').to_lowercase();
    let (matching_feeds, host, port, plugins_dir) = {
        let cfg = state.config.read().await;
        let list: Vec<(String, FeedConfig)> = cfg
            .feeds
            .iter()
            .filter(|(_, f)| {
                f.group
                    .as_ref()
                    .map(|g| g.to_lowercase() == clean_group)
                    .unwrap_or(false)
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        (
            list,
            cfg.server.host.clone(),
            cfg.server.port,
            cfg.plugins_dir.clone(),
        )
    };

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
        &host,
        port,
        &plugins_dir,
    )
    .await
}

async fn render_aggregated_feed(
    state: &Arc<AppState>,
    feed_title: &str,
    feeds_to_include: &[(String, FeedConfig)],
    host: &str,
    port: u16,
    plugins_dir: &str,
) -> Response {
    if feeds_to_include.is_empty() {
        let empty_feed = crate::model::Feed {
            title: feed_title.to_string(),
            link: format!("http://{}:{}/feeds", host, port),
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
        // Skip NOP feeds in aggregate view so dead feeds don't clutter the timeline
        if feed_cfg.nop {
            continue;
        }

        let mut feed = match state.fetcher.fetch(&feed_cfg.url).await {
            Ok(f) => f,
            Err(e) => {
                // Skip dead feeds in the aggregated view instead of failing
                error!("Skipping failed feed in aggregate view {}: {}", slug, e);
                continue;
            }
        };

        if let Some(ref override_title) = feed_cfg.title_override {
            feed.title = override_title.clone();
        }

        let pipeline = match build_pipeline_for_feed(feed_cfg, plugins_dir) {
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
        link: format!("http://{}:{}/feeds", host, port),
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

    let (feed_cfg, plugins_dir) = {
        let cfg = state.config.read().await;
        match cfg.feeds.get(clean_slug) {
            Some(c) => (c.clone(), cfg.plugins_dir.clone()),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    format!("Feed with slug '{}' not found in config", clean_slug),
                )
                    .into_response();
            }
        }
    };

    let feed_title = feed_cfg.title_override.as_deref().unwrap_or(clean_slug);

    // 0. If marked as NOP in config, serve NOP feed immediately without fetching upstream
    if feed_cfg.nop {
        let reason = feed_cfg
            .nop_reason
            .as_deref()
            .unwrap_or("Marked as NOP in configuration");
        let nop_feed = NopHandlerPlugin::make_nop_feed(&feed_cfg.url, feed_title, reason);
        return match nop_feed.to_rss_xml() {
            Ok(xml) => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
                xml,
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        };
    }

    // 1. Build pipeline
    let pipeline = match build_pipeline_for_feed(&feed_cfg, &plugins_dir) {
        Ok(p) => p,
        Err(e) => {
            error!("Invalid config for feed {}: {}", clean_slug, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
        }
    };

    // 2. Fetch feed (with automatic NOP fallback if upstream fails)
    let mut feed = match state.fetcher.fetch(&feed_cfg.url).await {
        Ok(f) => f,
        Err(e) => {
            if let Some(nop_feed) = pipeline
                .handle_fetch_error(&feed_cfg.url, feed_title, &e)
                .await
            {
                return match nop_feed.to_rss_xml() {
                    Ok(xml) => (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
                        xml,
                    )
                        .into_response(),
                    Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
                };
            }

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
        return (
            StatusCode::BAD_REQUEST,
            "Missing required 'url' query parameter",
        )
            .into_response();
    }

    let mut pipeline = PluginPipeline::new();

    // Default cleaner & NOP fallback
    pipeline.add_plugin(Box::new(FeedCleanerPlugin::new()));
    pipeline.add_plugin(Box::new(NopHandlerPlugin::new()));

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
        pipeline.add_plugin(Box::new(KeywordFilterPlugin::new(
            include_kws.clone(),
            exclude_kws.clone(),
        )));
    }

    // Regex
    let include_regex = params.include_regex.map(|r| vec![r]).unwrap_or_default();
    let exclude_regex = params.exclude_regex.map(|r| vec![r]).unwrap_or_default();

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
    let plugins_dir = {
        let cfg = state.config.read().await;
        cfg.plugins_dir.clone()
    };

    if let Some(ref script_name) = params.script {
        let script_file = if script_name.ends_with(".rhai") {
            script_name.clone()
        } else {
            format!("{}.rhai", script_name)
        };
        let path = PathBuf::from(&plugins_dir).join(&script_file);
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

    // Fetch feed (with automatic fallback to NOP feed on error)
    let mut feed = match state.fetcher.fetch(&params.url).await {
        Ok(f) => f,
        Err(e) => match pipeline
            .handle_fetch_error(&params.url, &params.url, &e)
            .await
        {
            Some(nop_feed) => nop_feed,
            None => {
                return (
                    StatusCode::BAD_GATEWAY,
                    format!("Failed to fetch upstream feed: {}", e),
                )
                    .into_response();
            }
        },
    };

    // Auto-register in config if it doesn't exist
    let auto_register = {
        let cfg = state.config.read().await;
        cfg.auto_register_proxy_feeds
    };

    let mut registered_slug = None;

    if auto_register {
        let mut cfg = state.config.write().await;
        let existing = cfg
            .feeds
            .iter()
            .find(|(_, f)| f.url == params.url)
            .map(|(s, _)| s.clone());
        if let Some(slug) = existing {
            registered_slug = Some(slug);
        } else {
            let base_slug = crate::opml::slugify(&feed.title);
            let existing_slugs: HashSet<String> = cfg.feeds.keys().cloned().collect();
            let slug = crate::opml::unique_slug(&base_slug, &existing_slugs);

            let feed_config = FeedConfig {
                url: params.url.clone(),
                title_override: Some(feed.title.clone()),
                group: params.group.clone().or_else(|| Some("Proxied".to_string())),
                nop: false,
                nop_reason: None,
                filters: FeedFilterConfig {
                    include_keywords: include_kws,
                    exclude_keywords: exclude_kws,
                    include_regex,
                    exclude_regex,
                    clean_links: params.clean_links.unwrap_or(true),
                    max_age_days: params.max_age_days,
                    max_items: params.limit,
                    scripts: params.script.into_iter().collect(),
                },
            };

            cfg.feeds.insert(slug.clone(), feed_config);
            registered_slug = Some(slug.clone());

            if let Ok(yaml) = serde_yaml::to_string(&*cfg) {
                if let Err(e) = std::fs::write(&state.config_path, yaml) {
                    error!(
                        "Failed to persist auto-registered feed to {:?}: {}",
                        state.config_path, e
                    );
                } else {
                    info!(
                        "Auto-registered new feed '{}' as '/feeds/{}' in {:?}",
                        feed.title, slug, state.config_path
                    );
                }
            }
        }
    }

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
        Ok(xml) => {
            let mut response = (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
                xml,
            )
                .into_response();

            if let Some(slug) = registered_slug {
                if let Ok(val) = HeaderValue::from_str(&slug) {
                    response.headers_mut().insert("X-Feed-Slug", val);
                }
            }

            response
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("XML generation error: {}", e),
        )
            .into_response(),
    }
}
