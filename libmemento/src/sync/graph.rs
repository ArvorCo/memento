use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::sync::{SyncDocument, WikiLink};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VaultNode {
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub outlinks: Vec<String>,
    pub inlinks: Vec<String>,
    pub pagerank: f64,
    pub is_hub: bool,
    pub community_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeadLink {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VaultGraph {
    pub nodes: HashMap<String, VaultNode>,
    pub dead_links: Vec<DeadLink>,
}

impl VaultGraph {
    pub fn build(documents: &[SyncDocument]) -> Self {
        let mut nodes = HashMap::new();
        let mut exact_index = HashMap::new();
        let mut stem_index = HashMap::new();

        for document in documents {
            let canonical = canonical_path(&document.path);
            exact_index.insert(canonical.clone(), document.path.clone());

            let stem_key = stem_key(&document.path);
            stem_index
                .entry(stem_key)
                .or_insert_with(|| document.path.clone());

            nodes.insert(
                document.path.clone(),
                VaultNode {
                    path: document.path.clone(),
                    title: document.title.clone(),
                    tags: document.tags.clone(),
                    outlinks: Vec::new(),
                    inlinks: Vec::new(),
                    pagerank: 0.0,
                    is_hub: is_hub_path(&document.path),
                    community_id: None,
                },
            );
        }

        let mut dead_links = Vec::new();

        for document in documents {
            let links = if document.links.is_empty() {
                extract_wikilinks(&document.path, &document.content)
            } else {
                document.links.clone()
            };

            let mut resolved_targets = HashSet::new();
            for link in links {
                if let Some(resolved) = resolve_link(&link.target, &exact_index, &stem_index) {
                    resolved_targets.insert(resolved);
                } else {
                    dead_links.push(DeadLink {
                        source: document.path.clone(),
                        target: link.target.clone(),
                    });
                }
            }

            if let Some(node) = nodes.get_mut(&document.path) {
                let mut outlinks: Vec<_> = resolved_targets.into_iter().collect();
                outlinks.sort();
                node.outlinks = outlinks;
            }
        }

        let outbound_pairs: Vec<(String, Vec<String>)> = nodes
            .iter()
            .map(|(path, node)| (path.clone(), node.outlinks.clone()))
            .collect();

        for (source, targets) in outbound_pairs {
            for target in targets {
                if let Some(node) = nodes.get_mut(&target) {
                    node.inlinks.push(source.clone());
                }
            }
        }

        for node in nodes.values_mut() {
            node.inlinks.sort();
            node.inlinks.dedup();
        }

        let pagerank = compute_pagerank(&nodes, 20, 0.85);
        for (path, score) in pagerank {
            if let Some(node) = nodes.get_mut(&path) {
                node.pagerank = score;
            }
        }

        dead_links.sort_by(|left, right| {
            left.source
                .cmp(&right.source)
                .then_with(|| left.target.cmp(&right.target))
        });

        Self { nodes, dead_links }
    }

    pub fn pagerank(&self) -> Vec<(String, f64)> {
        let mut ranks: Vec<_> = self
            .nodes
            .values()
            .map(|node| (node.path.clone(), node.pagerank))
            .collect();
        ranks.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        ranks
    }

    pub fn orphans(&self) -> Vec<&VaultNode> {
        let mut nodes: Vec<_> = self
            .nodes
            .values()
            .filter(|node| node.inlinks.is_empty())
            .collect();
        nodes.sort_by(|left, right| left.path.cmp(&right.path));
        nodes
    }

    pub fn dead_links(&self) -> &[DeadLink] {
        &self.dead_links
    }
}

pub fn extract_wikilinks(source: &str, content: &str) -> Vec<WikiLink> {
    let Some(regex) = wikilink_regex() else {
        return Vec::new();
    };

    regex
        .captures_iter(content)
        .filter_map(|captures| captures.get(1).map(|matched| matched.as_str().trim()))
        .filter(|raw| !raw.is_empty())
        .map(|raw| parse_wikilink(source, raw))
        .collect()
}

fn parse_wikilink(source: &str, raw: &str) -> WikiLink {
    let (left, right) = raw
        .split_once('|')
        .map_or((raw, None), |(target, display)| (target, Some(display)));
    let (target, anchor_from_target) = left
        .split_once('#')
        .map_or((left, None), |(target, anchor)| {
            (target, Some(anchor.trim()))
        });

    let (display, anchor_from_display) = match right {
        Some(display) => match display.split_once('#') {
            Some((display, anchor)) if anchor_from_target.is_none() => (
                Some(display.trim().to_owned()),
                Some(anchor.trim().to_owned()),
            ),
            _ => (Some(display.trim().to_owned()), None),
        },
        None => (None, None),
    };

    WikiLink {
        source: source.to_owned(),
        target: target.trim().to_owned(),
        display,
        anchor: anchor_from_target
            .map(str::to_owned)
            .or(anchor_from_display),
    }
}

fn resolve_link(
    target: &str,
    exact_index: &HashMap<String, String>,
    stem_index: &HashMap<String, String>,
) -> Option<String> {
    let canonical = canonical_target(target);
    if let Some(path) = exact_index.get(&canonical) {
        return Some(path.clone());
    }

    stem_index.get(&stem_only(&canonical)).cloned()
}

fn compute_pagerank(
    nodes: &HashMap<String, VaultNode>,
    iterations: usize,
    damping: f64,
) -> HashMap<String, f64> {
    if nodes.is_empty() {
        return HashMap::new();
    }

    let node_count = nodes.len() as f64;
    let base = (1.0 - damping) / node_count;
    let mut ranks: HashMap<String, f64> = nodes
        .keys()
        .map(|path| (path.clone(), 1.0 / node_count))
        .collect();

    for _ in 0..iterations {
        let mut next: HashMap<String, f64> =
            nodes.keys().map(|path| (path.clone(), base)).collect();
        let sink_total: f64 = nodes
            .values()
            .filter(|node| node.outlinks.is_empty())
            .map(|node| *ranks.get(&node.path).unwrap_or(&0.0))
            .sum();

        if sink_total > 0.0 {
            let sink_share = damping * sink_total / node_count;
            for value in next.values_mut() {
                *value += sink_share;
            }
        }

        for node in nodes.values() {
            if node.outlinks.is_empty() {
                continue;
            }

            let rank = *ranks.get(&node.path).unwrap_or(&0.0);
            let share = damping * rank / node.outlinks.len() as f64;
            for target in &node.outlinks {
                if let Some(value) = next.get_mut(target) {
                    *value += share;
                }
            }
        }

        ranks = next;
    }

    ranks
}

fn wikilink_regex() -> Option<&'static Regex> {
    static WIKILINK_REGEX: OnceLock<Option<Regex>> = OnceLock::new();
    WIKILINK_REGEX
        .get_or_init(|| Regex::new(r"\[\[([^\]]+)\]\]").ok())
        .as_ref()
}

fn canonical_target(target: &str) -> String {
    canonical_path(target)
}

fn canonical_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/").trim().to_lowercase();
    if normalized.ends_with(".md") {
        normalized.truncate(normalized.len() - 3);
    }
    normalized.trim_matches('/').to_owned()
}

fn stem_key(path: &str) -> String {
    let canonical = canonical_path(path);
    stem_only(&canonical)
}

fn stem_only(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .or_else(|| Path::new(path).file_name())
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| path.to_lowercase())
}

fn is_hub_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("hub") || lower.starts_with("moc - ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::SyncSourceType;

    fn document(path: &str, content: &str) -> SyncDocument {
        SyncDocument {
            path: path.into(),
            content: content.into(),
            frontmatter: None,
            title: Path::new(path)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            tags: vec![],
            links: vec![],
            source_type: SyncSourceType::ObsidianVault,
            mtime: 0,
        }
    }

    #[test]
    fn extract_wikilinks_parses_alias_and_anchor() {
        let links = extract_wikilinks(
            "notes/source.md",
            "Link [[Target Page|Alias Text#section]] and [[second/path#heading]].",
        );

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "Target Page");
        assert_eq!(links[0].display.as_deref(), Some("Alias Text"));
        assert_eq!(links[0].anchor.as_deref(), Some("section"));
        assert_eq!(links[1].target, "second/path");
        assert_eq!(links[1].anchor.as_deref(), Some("heading"));
    }

    #[test]
    fn build_resolves_links_by_lowercase_stem_and_reports_dead_links() {
        let graph = VaultGraph::build(&[
            document("People/Alice.md", "See [[bob]] and [[ghost]]."),
            document("teams/Bob.md", "Mention [[Alice]]."),
        ]);

        let alice = graph.nodes.get("People/Alice.md").unwrap();
        let bob = graph.nodes.get("teams/Bob.md").unwrap();

        assert_eq!(alice.outlinks, vec!["teams/Bob.md"]);
        assert_eq!(bob.inlinks, vec!["People/Alice.md"]);
        assert_eq!(
            graph.dead_links(),
            &[DeadLink {
                source: "People/Alice.md".into(),
                target: "ghost".into(),
            }]
        );
    }

    #[test]
    fn pagerank_and_orphans_reflect_graph_structure() {
        let graph = VaultGraph::build(&[
            document("hub.md", "[[child-a]] [[child-b]]"),
            document("child-a.md", "[[hub]]"),
            document("child-b.md", ""),
            document("lonely.md", ""),
        ]);

        let ranks = graph.pagerank();
        let orphans: Vec<_> = graph
            .orphans()
            .into_iter()
            .map(|node| node.path.clone())
            .collect();

        assert_eq!(ranks[0].0, "hub.md");
        assert!(orphans.contains(&"lonely.md".to_string()));
        assert!(!orphans.contains(&"hub.md".to_string()));
    }
}
