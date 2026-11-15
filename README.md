# RSS Proxy Server

A high-performance, extensible RSS/Atom proxy and filtering server written in Rust with dynamic plugin support.

It fetches upstream RSS/Atom/JSON feeds, parses them, applies configurable filtering and content transformation pipelines (including built-in filters and user-authored script plugins), and serves clean RSS 2.0 feeds to your RSS reader (NetNewsWire, Reeder, FreshRSS, Miniflux, etc.).

---

## Features

- **Blazing Fast & Low Memory**: Built with Rust, Axum, and Tokio. Consumes < 15MB RAM with sub-millisecond filtering times.
- **Universal Feed Support**: Automatically parses RSS 0.9x, 1.0, 2.0, Atom 0.3/1.0, and JSON Feed formats via `feed-rs`.
- **Extensible Plugin System**:
  - **Embedded Script Plugins (Rhai)**: Drop `.rhai` script files into the `plugins/` directory to write custom filter and mutation rules on the fly with no compilation or server restart required.
  - **Native Rust Plugins**: Extensible `Plugin` trait for compiled pipeline hooks.
- **Built-in Filters**:
  - **Keyword Filter**: Case-insensitive include and exclude matching across title, description, content, author, and categories.
  - **Regex Filter**: Match complex patterns across all feed fields.
  - **Link Cleaner**: Automatically strips tracking queries (`utm_source`, `utm_medium`, `fbclid`, `gclid`, etc.) from article links and GUIDs.
  - **Age Filter**: Automatically drops articles older than a specified number of days or hours.
  - **Item Limiter**: Caps maximum items in the output feed.
- **Three Routing Modes**:
  - **Aggregated All-in-One Feed**: Subscribe to `http://localhost:8080/feeds` to get posts from all configured feeds in a single chronological stream.
  - **Folder / Group Feeds**: Subscribe to `http://localhost:8080/groups/:group` (e.g. `/groups/business` or `/groups/Engineering Blogs`) to get aggregated posts for a specific category.
  - **Config-Driven Presets**: Define clean, permanent URLs like `http://localhost:8080/feeds/tech-news` in `config.yaml`.
- **Dynamic On-the-Fly Proxy with Auto-Registration**:
  - Subscribe directly via URL query parameters: `http://localhost:8080/proxy?url=https://example.com/feed.xml&exclude=crypto,sponsored`.
  - **Auto-Registration**: Any proxied feed URL requested via `/proxy` that is not already in `config.yaml` is automatically saved to `config.yaml` with a clean persistent slug (e.g. `/feeds/example-feed`).
  - Returns `X-Feed-Slug` in the response headers with the newly assigned slug.
- **Smart Caching**: In-memory cache with configurable TTL, HTTP `ETag`, and `If-Modified-Since` handling to minimize upstream bandwidth and avoid rate limits.

---

## Architecture

```
[RSS Client] ────────► [Axum HTTP Router]
                             │
                             ├── GET /feeds/:slug  (Config-driven presets)
                             ├── GET /proxy?url=   (Dynamic query proxy)
                             └── GET /plugins      (List loaded plugins)
                                     │
                             ▼
                   [Feed Fetcher + Cache]
                   (ETags, Last-Modified, TTL)
                             │
                             ▼
                    [feed-rs Parser]
                             │
                             ▼
                   [Plugin Pipeline]
        ┌──────────────────────────────────────────────┐
        │ 1. pre_process_feed()                        │
        │ 2. filter_item() -> Keep / Drop              │
        │    - Built-in KeywordFilterPlugin            │
        │    - Built-in RegexFilterPlugin              │
        │    - Built-in AgeFilterPlugin                │
        │    - Rhai Script Plugins (*.rhai)            │
        │ 3. transform_item() -> Mutate FeedItem       │
        │    - Built-in LinkCleanerPlugin (Strip UTMs) │
        │    - Rhai Script Plugins (*.rhai)            │
        │ 4. post_process_feed()                       │
        │    - Built-in LimitPlugin                    │
        └──────────────────────────────────────────────┘
                             │
                             ▼
                    [RSS 2.0 Generator]
                             │
[Clean RSS Feed] ◄───────────┘
```

---

## Quickstart

### 1. Build and Run

```bash
# Build the project
cargo build --release

# Run with default configuration (reads config.yaml on port 8080)
./target/release/rss-proxy

# Or override port and config
./target/release/rss-proxy --config config.yaml --port 9090
```

### 2. Verify Server Status

```bash
curl http://localhost:8080/
```

Returns:
```json
{
  "status": "running",
  "version": "0.1.0",
  "configured_feeds": ["tech-news", "rust-releases"],
  "available_plugins": ["filter_sponsors", "tag_and_prefix"]
}
```

---

## OPML Import & Export

### 1. Import an OPML File into `config.yaml`

Import your existing subscriptions from NetNewsWire, Reeder, Feedly, etc.:

```bash
# Preview discovered feeds without modifying config
cargo run --release -- import-opml /path/to/subscriptions.opml --dry-run

# Import and merge into config.yaml
cargo run --release -- import-opml /path/to/subscriptions.opml
```

The importer automatically:
- Recursively parses folder structures and feed entries
- Generates clean, unique, URL-safe slugs (`Hacker News` -> `hacker-news`)
- Preserves existing configured feeds and server settings in `config.yaml`
- Adds link-cleaning by default, ready for your custom keywords/regex/plugins

### 2. Export Proxied Feeds to OPML (For Your RSS Reader)

Export all your configured proxied feeds as an OPML file pointing to the proxy server:

```bash
# Export to a file
cargo run --release -- export-opml --output proxied-subscriptions.opml

# Or specify a custom base URL
cargo run --release -- export-opml --base-url http://192.168.1.100:8080 -o subscriptions.opml
```

You can then import `proxied-subscriptions.opml` directly into your RSS reader.

### 3. Dynamic OPML HTTP Endpoint

While the server is running, you can also fetch your proxied subscriptions at:
```
http://localhost:8080/opml
```

---

## Configuration (`config.yaml`)

Define feeds and pipeline rules in `config.yaml`:

```yaml
server:
  host: "127.0.0.1"
  port: 8080

cache_ttl_seconds: 300 # Cache upstream responses for 5 minutes
plugins_dir: "./plugins"

feeds:
  tech-news:
    url: "https://hnrss.org/frontpage"
    title_override: "Hacker News (Filtered)"
    filters:
      exclude_keywords:
        - "crypto"
        - "nft"
        - "sponsored"
      clean_links: true
      max_items: 25
      scripts:
        - "filter_sponsors" # Refers to plugins/filter_sponsors.rhai

  rust-releases:
    url: "https://blog.rust-lang.org/feed.xml"
    title_override: "Rust Official Releases"
    filters:
      include_regex:
        - "(?i)announcing rust"
      clean_links: true
      max_age_days: 60
```

Subscribe your RSS reader to:
`http://localhost:8080/feeds/tech-news`

---

## Dynamic Query Parameter Proxy

You can also proxy feeds dynamically without editing `config.yaml`:

```
http://localhost:8080/proxy?url=<FEED_URL>&exclude=<KW1,KW2>&include=<KW>&clean_links=true&limit=20&script=<SCRIPT_NAME>
```

### Supported Parameters:

| Parameter | Type | Description |
| :--- | :--- | :--- |
| `url` | String (Required) | The upstream feed URL |
| `exclude` | Comma-separated | Keywords that trigger item removal |
| `include` | Comma-separated | At least one keyword must be present |
| `exclude_regex` | Regex | Regex pattern that triggers item removal |
| `include_regex` | Regex | Regex pattern that items must match |
| `clean_links` | Boolean (default: `true`) | Strips `utm_*`, `fbclid`, and analytics query params |
| `max_age_days` | Integer | Drops items older than N days |
| `limit` | Integer | Limits maximum number of output entries |
| `script` | String | Name of a Rhai plugin in `plugins/` |

**Example:**
```
http://localhost:8080/proxy?url=https://hnrss.org/frontpage&exclude=show%20hn,crypto&clean_links=true&limit=15
```

---

## Developing Plugins

Plugins allow you to create custom filtering and transformation logic.

### 1. Rhai Script Plugins (No Compilation Needed)

Drop any `.rhai` script into the `plugins/` directory.

#### Available Properties on `item`:
- `item.id`: Unique GUID or link (string)
- `item.title`: Article title (string)
- `item.link`: Article link (string)
- `item.description`: Article summary/snippet (string)
- `item.content`: Full HTML/text if available (string)
- `item.author`: Author name (string)
- `item.categories`: Array of tags/categories (`["tech", "news"]`)

#### Example: `plugins/filter_sponsors.rhai`
```rhai
// Drop articles matching custom conditions
fn filter_item(item) {
    let lower_title = item.title.to_lower();
    let lower_desc = item.description.to_lower();

    if lower_title.contains("sponsored") || lower_desc.contains("sponsored by") {
        return "drop"; // or return false;
    }

    return "keep"; // or return true;
}
```

#### Example: `plugins/tag_and_prefix.rhai`
```rhai
// Mutate or sanitize articles
fn transform_item(item) {
    let lower_title = item.title.to_lower();

    if lower_title.contains("rust") && !item.title.starts_with("[Rust]") {
        item.title = "[Rust] " + item.title;
    }

    return item;
}
```

### 2. Native Rust Plugins

Implement the `Plugin` trait in `src/plugin/mod.rs`:

```rust
use async_trait::async_trait;
use crate::model::{Feed, FeedItem};
use crate::plugin::{FilterDecision, Plugin};

pub struct MyCustomPlugin;

#[async_trait]
impl Plugin for MyCustomPlugin {
    fn name(&self) -> &str {
        "MyCustomPlugin"
    }

    async fn filter_item(&self, item: &FeedItem) -> anyhow::Result<FilterDecision> {
        if item.title.starts_with("SPAM:") {
            return Ok(FilterDecision::Drop);
        }
        Ok(FilterDecision::Keep)
    }

    async fn transform_item(&self, item: &mut FeedItem) -> anyhow::Result<()> {
        item.title = format!("⭐ {}", item.title);
        Ok(())
    }
}
```

---

## Running the Test Suite

```bash
cargo test
```

Runs all 12 unit and integration tests covering:
- Keyword inclusion and exclusion
- Regex pattern matching
- Tracking parameter stripping from links and GUIDs
- Max age filtering
- Feed item truncation
- Rhai script loading and execution (`filter_item` and `transform_item`)
- End-to-end multi-stage pipeline processing
- Axum router endpoints (`/health`, `/`, `/plugins`, `/proxy`)
