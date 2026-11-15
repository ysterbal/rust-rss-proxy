use anyhow::{anyhow, Result};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use tracing::info;

use crate::config::{AppConfig, FeedConfig, FeedFilterConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpmlFeed {
    pub title: String,
    pub xml_url: String,
    pub html_url: Option<String>,
    pub category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportableFeed {
    pub title: String,
    pub xml_url: String,
    pub html_url: Option<String>,
    pub group: Option<String>,
}

pub fn parse_opml(xml_content: &str) -> Result<Vec<OpmlFeed>> {
    let mut reader = Reader::from_str(xml_content);
    reader.trim_text(true);

    let mut feeds = Vec::new();
    let mut folder_stack: Vec<String> = Vec::new();
    let mut node_stack: Vec<bool> = Vec::new(); // true if folder, false if feed/other

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"outline" => {
                let (feed, is_folder) = parse_outline_attributes(e, &folder_stack)?;
                if let Some(f) = feed {
                    feeds.push(f);
                    node_stack.push(false);
                } else if let Some(folder_name) = is_folder {
                    folder_stack.push(folder_name);
                    node_stack.push(true);
                } else {
                    node_stack.push(false);
                }
            }
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"outline" => {
                let (feed, _) = parse_outline_attributes(e, &folder_stack)?;
                if let Some(f) = feed {
                    feeds.push(f);
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"outline" => {
                if let Some(was_folder) = node_stack.pop() {
                    if was_folder {
                        folder_stack.pop();
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow!("OPML XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(feeds)
}

fn parse_outline_attributes(
    e: &quick_xml::events::BytesStart,
    folder_stack: &[String],
) -> Result<(Option<OpmlFeed>, Option<String>)> {
    let mut title = None;
    let mut text = None;
    let mut xml_url = None;
    let mut html_url = None;

    for attr in e.attributes() {
        let attr = attr.map_err(|err| anyhow!("XML attribute error: {}", err))?;
        let key = std::str::from_utf8(attr.key.as_ref())?.to_lowercase();
        let raw_val = String::from_utf8_lossy(&attr.value);
        let value = match quick_xml::escape::unescape(&raw_val) {
            Ok(v) => v.into_owned(),
            Err(_) => raw_val.into_owned(),
        };

        match key.as_str() {
            "title" => title = Some(value),
            "text" => text = Some(value),
            "xmlurl" => xml_url = Some(value),
            "htmlurl" => html_url = Some(value),
            _ => {}
        }
    }

    let feed_title = title.clone().or(text.clone()).unwrap_or_else(|| "Untitled Feed".to_string());

    if let Some(url) = xml_url {
        let category = folder_stack.last().cloned();
        Ok((
            Some(OpmlFeed {
                title: feed_title,
                xml_url: url,
                html_url,
                category,
            }),
            None,
        ))
    } else {
        // Outline without xmlUrl is a category folder
        let folder_name = text.or(title).unwrap_or_else(|| "Folder".to_string());
        Ok((None, Some(folder_name)))
    }
}

pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;

    for ch in title.chars() {
        if ch.is_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }

    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "feed".to_string()
    } else {
        trimmed
    }
}

pub fn unique_slug(base: &str, existing: &HashSet<String>) -> String {
    if !existing.contains(base) {
        return base.to_string();
    }
    let mut counter = 2;
    loop {
        let candidate = format!("{}-{}", base, counter);
        if !existing.contains(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

pub fn import_opml_into_config<P1: AsRef<Path>, P2: AsRef<Path>>(
    opml_path: P1,
    config_path: P2,
    dry_run: bool,
) -> Result<Vec<(String, OpmlFeed)>> {
    let content = std::fs::read_to_string(opml_path.as_ref())
        .map_err(|e| anyhow!("Failed to read OPML file: {}", e))?;

    let feeds = parse_opml(&content)?;
    if feeds.is_empty() {
        return Ok(Vec::new());
    }

    let mut config = AppConfig::load_or_default(config_path.as_ref())?;
    let mut existing_slugs: HashSet<String> = config.feeds.keys().cloned().collect();
    let mut imported = Vec::new();

    for feed in feeds {
        let base_slug = slugify(&feed.title);
        let slug = unique_slug(&base_slug, &existing_slugs);
        existing_slugs.insert(slug.clone());

        let feed_config = FeedConfig {
            url: feed.xml_url.clone(),
            title_override: Some(feed.title.clone()),
            group: feed.category.clone(),
            filters: FeedFilterConfig {
                clean_links: true,
                ..Default::default()
            },
        };

        config.feeds.insert(slug.clone(), feed_config);
        imported.push((slug, feed));
    }

    if !dry_run {
        let yaml = serde_yaml::to_string(&config)
            .map_err(|e| anyhow!("Failed to serialize config to YAML: {}", e))?;
        std::fs::write(config_path.as_ref(), yaml)
            .map_err(|e| anyhow!("Failed to save updated config: {}", e))?;
        info!("Updated config file at {:?}", config_path.as_ref());
    }

    Ok(imported)
}

/// Generates valid OPML 2.0 preserving category folder/outline hierarchy
pub fn generate_grouped_opml(title: &str, feeds: &[ExportableFeed]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<opml version=\"2.0\">\n");
    out.push_str("  <head>\n");
    out.push_str(&format!("    <title>{}</title>\n", quick_xml::escape::escape(title)));
    out.push_str("  </head>\n");
    out.push_str("  <body>\n");

    // Group feeds by category
    let mut grouped: BTreeMap<Option<String>, Vec<&ExportableFeed>> = BTreeMap::new();
    for f in feeds {
        grouped.entry(f.group.clone()).or_default().push(f);
    }

    // First write feeds with folders
    for (group_opt, group_feeds) in &grouped {
        if let Some(ref group_name) = group_opt {
            let escaped_group = quick_xml::escape::escape(group_name);
            out.push_str(&format!(
                "    <outline text=\"{}\" title=\"{}\">\n",
                escaped_group, escaped_group
            ));

            for f in group_feeds {
                write_feed_outline(&mut out, f, "      ");
            }

            out.push_str("    </outline>\n");
        }
    }

    // Then write uncategorized/top-level feeds
    if let Some(top_level) = grouped.get(&None) {
        for f in top_level {
            write_feed_outline(&mut out, f, "    ");
        }
    }

    out.push_str("  </body>\n");
    out.push_str("</opml>\n");
    out
}

fn write_feed_outline(out: &mut String, feed: &ExportableFeed, indent: &str) {
    let escaped_title = quick_xml::escape::escape(&feed.title);
    let escaped_xml = quick_xml::escape::escape(&feed.xml_url);
    let html_attr = match feed.html_url {
        Some(ref u) => format!(" htmlUrl=\"{}\"", quick_xml::escape::escape(u)),
        None => String::new(),
    };

    out.push_str(&format!(
        "{}<outline type=\"rss\" text=\"{}\" title=\"{}\" xmlUrl=\"{}\"{}/>\n",
        indent, escaped_title, escaped_title, escaped_xml, html_attr
    ));
}
