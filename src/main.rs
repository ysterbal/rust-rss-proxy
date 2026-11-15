use clap::{Parser, Subcommand};
use rss_proxy::{config, fetcher, opml, server};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(
    name = "rss-proxy",
    about = "High-performance extensible RSS/Atom proxy and filtering server"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to YAML configuration file
    #[arg(short, long, default_value = "config.yaml", global = true)]
    config: PathBuf,

    /// Override server port
    #[arg(short, long, global = true)]
    port: Option<u16>,

    /// Override server host
    #[arg(long, global = true)]
    host: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the RSS proxy server (default)
    Serve,
    /// Import feeds from an OPML subscription file into config.yaml
    ImportOpml {
        /// Path to OPML XML file
        file: PathBuf,
        /// Preview discovered feeds without modifying config.yaml
        #[arg(long)]
        dry_run: bool,
    },
    /// Export configured proxied feeds to an OPML file for your RSS reader
    ExportOpml {
        /// Base URL for proxy server (default: derived from config)
        #[arg(long)]
        base_url: Option<String>,
        /// Output file path (prints to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Test all configured feeds, identify dead/failing URLs, and optionally mark them as NOP in config.yaml
    AuditFeeds {
        /// Automatically mark failing/unreachable feeds as nop: true in config.yaml
        #[arg(long)]
        mark_nop: bool,
        /// Request timeout in seconds per feed
        #[arg(long, default_value_t = 8)]
        timeout: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rss_proxy=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Serve) {
        Commands::Serve => run_server(cli.config, cli.host, cli.port).await?,
        Commands::ImportOpml { file, dry_run } => {
            println!("Importing OPML feeds from: {}", file.display());
            let imported = opml::import_opml_into_config(&file, &cli.config, dry_run)?;

            println!("\nDiscovered {} feed(s):", imported.len());
            for (slug, feed) in &imported {
                let cat_str = feed
                    .category
                    .as_deref()
                    .map(|c| format!(" [{}]", c))
                    .unwrap_or_default();
                println!("  • {} -> /feeds/{}{}", feed.title, slug, cat_str);
                println!("    Upstream: {}", feed.xml_url);
            }

            if dry_run {
                println!("\n(Dry run: no changes written to {})", cli.config.display());
            } else {
                println!("\nSuccessfully imported {} feed(s) into {}", imported.len(), cli.config.display());
            }
        }
        Commands::ExportOpml { base_url, output } => {
            let config = config::AppConfig::load_or_default(&cli.config)?;
            let host_url = base_url.unwrap_or_else(|| {
                format!("http://{}:{}", config.server.host, config.server.port)
            });

            let feeds: Vec<opml::ExportableFeed> = config
                .feeds
                .iter()
                .map(|(slug, cfg)| {
                    let title = cfg
                        .title_override
                        .clone()
                        .unwrap_or_else(|| slug.clone());
                    let proxy_url = format!("{}/feeds/{}", host_url, slug);
                    opml::ExportableFeed {
                        title,
                        xml_url: proxy_url,
                        html_url: None,
                        group: cfg.group.clone(),
                    }
                })
                .collect();

            let opml_content = opml::generate_grouped_opml("RSS Proxy Subscriptions", &feeds);

            if let Some(out_path) = output {
                std::fs::write(&out_path, &opml_content)?;
                println!(
                    "Exported {} proxied feed(s) with folder hierarchy to {}",
                    feeds.len(),
                    out_path.display()
                );
            } else {
                print!("{}", opml_content);
            }
        }
        Commands::AuditFeeds { mark_nop, timeout } => {
            let mut config = config::AppConfig::load_or_default(&cli.config)?;
            println!("Auditing {} feed(s) in {} (timeout: {}s)...", config.feeds.len(), cli.config.display(), timeout);

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(timeout))
                .user_agent("rss-proxy/0.1.0 (+https://github.com/rust-rss-proxy)")
                .build()?;

            let mut healthy_count = 0;
            let mut failing_count = 0;
            let mut updated_config = false;

            for (slug, cfg) in config.feeds.iter_mut() {
                let title = cfg.title_override.as_deref().unwrap_or(slug);

                let check_result = match client.get(&cfg.url).send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        if status.is_success() {
                            match resp.bytes().await {
                                Ok(bytes) => match feed_rs::parser::parse(&bytes[..]) {
                                    Ok(parsed) => Ok(parsed.entries.len()),
                                    Err(err) => Err(format!("Invalid feed XML ({})", err)),
                                },
                                Err(err) => Err(format!("Failed to read body ({})", err)),
                            }
                        } else {
                            Err(format!("HTTP {}", status))
                        }
                    }
                    Err(err) => {
                        if err.is_timeout() {
                            Err("Timed out".to_string())
                        } else {
                            Err(format!("Network error ({})", err))
                        }
                    }
                };

                match check_result {
                    Ok(count) => {
                        healthy_count += 1;
                        if cfg.nop {
                            // Recovered feed!
                            println!("  [RECOVERED] {} ({} items)", title, count);
                            if mark_nop {
                                cfg.nop = false;
                                cfg.nop_reason = None;
                                updated_config = true;
                            }
                        } else {
                            println!("  [OK]        {} ({} items)", title, count);
                        }
                    }
                    Err(reason) => {
                        failing_count += 1;
                        println!("  [NOP]       {} - {} ({})", title, reason, cfg.url);
                        if mark_nop && (!cfg.nop || cfg.nop_reason.as_deref() != Some(&reason)) {
                            cfg.nop = true;
                            cfg.nop_reason = Some(reason);
                            updated_config = true;
                        }
                    }
                }
            }

            println!("\nAudit summary:");
            println!("  Healthy feeds: {}", healthy_count);
            println!("  Failing feeds: {}", failing_count);

            if mark_nop && updated_config {
                let yaml = serde_yaml::to_string(&config)?;
                std::fs::write(&cli.config, yaml)?;
                println!("\nUpdated {} with NOP flags.", cli.config.display());
            } else if !mark_nop && failing_count > 0 {
                println!("\nTip: Run with --mark-nop to automatically flag failing feeds in {}.", cli.config.display());
            }
        }
    }

    Ok(())
}

async fn run_server(
    config_path: PathBuf,
    host_override: Option<String>,
    port_override: Option<u16>,
) -> anyhow::Result<()> {
    info!("Starting rss-proxy...");

    let mut config = config::AppConfig::load_or_default(&config_path)?;

    if let Some(port) = port_override {
        config.server.port = port;
    }
    if let Some(host) = host_override {
        config.server.host = host;
    }

    if !std::path::Path::new(&config.plugins_dir).exists() {
        if let Err(e) = std::fs::create_dir_all(&config.plugins_dir) {
            tracing::warn!("Could not create plugins directory {}: {}", config.plugins_dir, e);
        }
    }

    let fetcher = fetcher::FeedFetcher::new(config.cache_ttl_seconds);
    let state = Arc::new(server::AppState {
        config: config.clone(),
        fetcher,
    });
    let app = server::create_router(state);

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("RSS Proxy server listening on http://{}", addr);
    info!("Configured feeds: {}", config.feeds.len());
    info!("Plugins directory: {}", config.plugins_dir);
    info!("OPML export available at: http://{}/opml", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
