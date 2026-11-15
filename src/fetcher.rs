use crate::model::Feed;
use anyhow::{anyhow, Context, Result};
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, USER_AGENT};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info};

#[derive(Clone)]
struct CacheEntry {
    feed: Feed,
    etag: Option<String>,
    last_modified: Option<String>,
    fetched_at: Instant,
}

#[derive(Clone)]
pub struct FeedFetcher {
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    ttl: Duration,
}

impl FeedFetcher {
    pub fn new(ttl_seconds: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("Failed to initialize reqwest client");

        Self {
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub async fn fetch(&self, url: &str) -> Result<Feed> {
        // 1. Check in-memory cache
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(url) {
                if entry.fetched_at.elapsed() < self.ttl {
                    debug!("Returning cached feed for {}", url);
                    return Ok(entry.feed.clone());
                }
            }
        }

        // 2. Prepare conditional GET request if we have ETag / Last-Modified
        let (cached_etag, cached_last_modified) = {
            let cache = self.cache.read().await;
            cache
                .get(url)
                .map(|e| (e.etag.clone(), e.last_modified.clone()))
                .unwrap_or((None, None))
        };

        let mut request = self
            .client
            .get(url)
            .header(USER_AGENT, "rss-proxy/0.1.0 (+https://github.com/rust-rss-proxy)");

        if let Some(ref etag) = cached_etag {
            request = request.header(IF_NONE_MATCH, etag);
        }
        if let Some(ref lm) = cached_last_modified {
            request = request.header(IF_MODIFIED_SINCE, lm);
        }

        info!("Fetching upstream feed from: {}", url);
        let response = request
            .send()
            .await
            .with_context(|| format!("Failed to send request to {}", url))?;

        // 3. Handle 304 Not Modified
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            debug!("Upstream returned 304 Not Modified for {}", url);
            let mut cache = self.cache.write().await;
            if let Some(entry) = cache.get_mut(url) {
                entry.fetched_at = Instant::now();
                return Ok(entry.feed.clone());
            }
        }

        if !response.status().is_success() {
            return Err(anyhow!(
                "Upstream returned HTTP error {} for {}",
                response.status(),
                url
            ));
        }

        // 4. Capture caching headers
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let last_modified = response
            .headers()
            .get(LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let bytes = response
            .bytes()
            .await
            .with_context(|| format!("Failed to read response body from {}", url))?;

        // 5. Parse feed with feed-rs
        let parsed = feed_rs::parser::parse(&bytes[..])
            .with_context(|| format!("Failed to parse feed XML from {}", url))?;

        let feed = Feed::from_feed_rs(parsed);

        // 6. Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                url.to_string(),
                CacheEntry {
                    feed: feed.clone(),
                    etag,
                    last_modified,
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok(feed)
    }
}
