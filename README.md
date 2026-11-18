# RSS Proxy

A high-performance, extensible RSS/Atom proxy and filtering server written in Rust. Route, filter, and transform feed content through a unified API with plugin-based extensibility.

[![Build Status](https://github.com/rust-rss-proxy/rss-proxy/actions/workflows/ci.yml/badge.svg)](https://github.com/rust-rss-proxy/rss-proxy/actions)
[![Version](https://img.shields.io/crates/v/rss-proxy.svg)](https://crates.io/crates/rss-proxy)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Table of Contents

- [Features](#features)
- [Why RSS Proxy?](#why-rss-proxy)
- [Quick Start](#quick-start)
  - [Installation](#installation)
    - [Pre-built Binaries](#pre-built-binaries)
    - [From Source](#from-source)
    - [Docker](#docker)
  - [Running the Server](#running-the-server)
- [Configuration](#configuration)
- [CLI Commands](#cli-commands)
  - [Serve](#serve)
  - [Import OPML](#import-opml)
  - [Export OPML](#export-opml)
  - [Audit Feeds](#audit-feeds)
- [API Endpoints](#api-endpoints)
- [Plugins and Filtering](#plugins-and-filtering)
- [Uninstall / Cleanup](#uninstall--cleanup)
- [Contributing](#contributing)
- [License](#license)

---

## Features

- **Proxy Feeds**: Fetch and serve RSS/Atom feeds through a unified proxy API
- **Dynamic Filtering**: Filter feeds by keywords, regex patterns, age, and item limits
- **Plugin Architecture**: Extend functionality with built-in plugins or custom Rhai scripts
- **OPML Import/Export**: Import subscriptions from OPML files and export proxied feeds
- **Feed Auditing**: Test configured feeds and identify dead/failing URLs
- **High Performance**: Built with Rust, tokio, and axum for fast, concurrent feed processing

## Why RSS Proxy?

Managing multiple RSS feeds across different sources often requires:
- Filtering out spam or unwanted content
- Grouping feeds by category for organized reading
- Handling feed URLs that change or require authentication proxies

**RSS Proxy differentiates itself by:**

| Feature | RSS Proxy | Other Proxies |
|---------|-----------|---------------|
| Plugin-based filtering | ✅ Rhai scripts + built-in plugins | ❌ Static config only |
| OPML import/export | ✅ Full support with folder hierarchy | ⚠️ Limited or none |
| Feed auditing | ✅ Test and auto-mark failing feeds | ❌ Manual testing required |
| Performance | ✅ Rust + tokio async | ⚠️ Python/Node.js overhead |
| Zero dependencies | ✅ Single binary release | ❌ Requires runtime environment |

---

## Quick Start

### Installation

#### Pre-built Binaries

Download the latest release from the [GitHub Releases page](https://github.com/rust-rss-proxy/rss-proxy/releases):

```bash
# Linux (x86_64)
curl -L https://github.com/rust-rss-proxy/rss-proxy/releases/latest/download/rss-proxy-x86_64-unknown-linux-gnu.tar.gz | tar -xz
sudo mv rss-proxy /usr/local/bin/

# macOS (x86_64 / arm64)
curl -L https://github.com/rust-rss-proxy/rss-proxy/releases/latest/download/rss-proxy-x86_64-apple-darwin.tar.gz | tar -xz
sudo mv rss-proxy /usr/local/bin/
```

#### From Source

Ensure you have Rust 1.75+ installed ([rustup.rs](https://rustup.rs/)), then:

```bash
git clone https://github.com/rust-rss-proxy/rss-proxy.git
cd rss-proxy
cargo build --release
cp target/release/rss-proxy /usr/local/bin/
```

#### Docker

Build and run using Docker Compose:

```bash
git clone https://github.com/rust-rss-proxy/rss-proxy.git
cd rss-proxy
docker-compose up -d
```

The proxy will be available at `http://localhost:8080`.

### Running the Server

Start the server with the default configuration (`config.yaml` on port 8080):

```bash
rss-proxy serve
```

Or specify custom host and port:

```bash
rss-proxy serve --host 0.0.0.0 --port 3000 --config /path/to/config.yaml
```

---

## Configuration

The server is configured via `config.yaml`. A sample configuration:

```yaml
server:
  host: 127.0.0.1
  port: 8080
cache_ttl_seconds: 300
plugins_dir: ./plugins
auto_register_proxy_feeds: true
feeds:
  my-feed:
    url: https://example.com/feed.xml
    title_override: My Custom Feed
    group: Technology
    nop: false
    nop_reason: null
    filters:
      include_keywords: []
      exclude_keywords: []
      include_regex: []
      exclude_regex: []
      clean_links: true
      max_age_days: null
      max_items: null
      scripts: []
```

---

## CLI Commands

### Serve

Start the RSS proxy server (default behavior):

```bash
rss-proxy serve
```

### Import OPML

Import feeds from an OPML subscription file into `config.yaml`:

```bash
# Preview discovered feeds without modifying config.yaml
rss-proxy import-opml subscriptions.opml --dry-run

# Import and update config.yaml
rss-proxy import-opml subscriptions.opml
```

### Export OPML

Export configured proxied feeds to an OPML file for your RSS reader:

```bash
# Export to stdout
rss-proxy export-opml

# Export to a file
rss-proxy export-opml --output proxied-subscriptions.opml

# Specify base URL
rss-proxy export-opml --base-url http://proxy.example.com --output proxied-subscriptions.opml
```

### Audit Feeds

Test all configured feeds, identify dead/failing URLs, and optionally mark them as NOP in `config.yaml`:

```bash
# Test feeds with default 8s timeout
rss-proxy audit-feeds

# Test with custom timeout and auto-mark failing feeds
rss-proxy audit-feeds --mark-nop --timeout 10
```

---

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | GET | Server status and configured feeds |
| `/health` | GET | Health check endpoint |
| `/plugins` | GET | List available plugins |
| `/opml` | GET | Export OPML of configured feeds |
| `/feeds` | GET | List all configured feeds |
| `/feeds/{slug}` | GET | Fetch a specific configured feed |
| `/groups/{group}` | GET | Fetch feeds grouped by category |
| `/proxy?url=<url>` | GET | Proxy a feed by URL with optional filters |

### Proxy Feed Example

```bash
# Proxy a feed with keyword filtering
curl "http://localhost:8080/proxy?url=https://example.com/feed.xml&exclude=spam&limit=10"

# Proxy with custom script
curl "http://localhost:8080/proxy?url=https://example.com/feed.xml&script=modify_title(item) { item.title = 'Custom: ' + item.title; item }"
```

---

## Plugins and Filtering

RSS Proxy supports both built-in plugins and custom Rhai scripts for feed transformation:

### Built-in Plugins

- **KeywordFilterPlugin**: Include/exclude items by keywords
- **RegexFilterPlugin**: Include/exclude items by regex patterns
- **AgeFilterPlugin**: Filter items by age (days)
- **LimitPlugin**: Limit the number of items returned
- **LinkCleanerPlugin**: Clean and normalize links in feed content
- **FeedCleanerPlugin**: General feed content cleaning
- **NopHandlerPlugin**: Mark feeds as disabled (NOP)

### Custom Rhai Scripts

Write custom Rhai scripts in the `plugins/` directory to transform feed items:

```rhai
// plugins/modify_title.rhai
export function modify_title(item) {
    if item.title.contains("SPAM:") {
        return null; // Filter out
    }
    item.title = "Featured: " + item.title;
    return item;
}
```

---

## Uninstall / Cleanup

### Removing the Binary

```bash
# If installed via cargo
cargo uninstall rss-proxy

# If installed manually
sudo rm /usr/local/bin/rss-proxy
```

### Removing Docker Containers

```bash
cd rss-proxy
docker-compose down
docker-compose rm -f
```

### Cleaning Configuration and Data

```bash
# Remove config and plugins (if no longer needed)
rm -f config.yaml
rm -rf plugins/
```

---

## Contributing

Contributions are welcome! Please follow these steps:

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Run the test suite locally: `cargo test`
4. Commit your changes: `git commit -am 'Add new feature'`
5. Push to the branch: `git push origin feature/my-feature`
6. Submit a pull request

### Running the Test Suite

```bash
cargo test
cargo test --release
```

For code style and PR guidelines, see [CONTRIBUTING.md](CONTRIBUTING.md).

---

## License

This project is licensed under the [MIT License](LICENSE).

```
MIT License

Copyright (c) 2024 RSS Proxy Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
