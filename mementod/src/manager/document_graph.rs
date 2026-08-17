use super::*;
use crate::text_utils::ascii_fold;
use libmemento::sync::graph::extract_wikilinks;

const RESTART_PROBABILITY: f64 = 0.25;
const WALK_STEPS: usize = 3;
const BACKLINK_WEIGHT: f64 = 0.35;

/// Rebuildable document-link graph used for query-local propagation.
///
/// The graph deliberately stays outside the canonical file format: wikilinks are
/// source data, so this view can be reconstructed after upgrades without a
/// migration. Edges are directed, with a weaker reverse edge for backlinks.
#[derive(Debug, Default)]
pub(super) struct DocumentGraph {
    adjacency: HashMap<DocId, Vec<(DocId, f64)>>,
    edge_count: usize,
}

impl DocumentGraph {
    pub fn build(documents: &[StoredDocument]) -> Self {
        let mut exact = HashMap::<String, DocId>::new();
        let mut stems = HashMap::<String, Vec<DocId>>::new();
        let mut paths = HashMap::<DocId, &str>::new();

        for document in documents {
            exact.insert(canonical_path(&document.source_path), document.doc_id);
            stems
                .entry(path_stem(&document.source_path))
                .or_default()
                .push(document.doc_id);
            paths.insert(document.doc_id, &document.source_path);
        }
        for doc_ids in stems.values_mut() {
            doc_ids.sort_unstable();
            doc_ids.dedup();
        }

        let mut weighted_edges = HashMap::<(DocId, DocId), f64>::new();
        for document in documents {
            if document.canonical_text.is_empty() {
                continue;
            }
            for link in extract_wikilinks(&document.source_path, &document.canonical_text) {
                let Some(target) =
                    resolve_target(&document.source_path, &link.target, &exact, &stems)
                else {
                    continue;
                };
                if target == document.doc_id {
                    continue;
                }
                weighted_edges
                    .entry((document.doc_id, target))
                    .and_modify(|weight| *weight = weight.max(1.0))
                    .or_insert(1.0);
                weighted_edges
                    .entry((target, document.doc_id))
                    .and_modify(|weight| *weight = weight.max(BACKLINK_WEIGHT))
                    .or_insert(BACKLINK_WEIGHT);
            }
        }

        let mut adjacency = HashMap::<DocId, Vec<(DocId, f64)>>::new();
        for ((source, target), weight) in weighted_edges {
            if paths.contains_key(&source) && paths.contains_key(&target) {
                adjacency.entry(source).or_default().push((target, weight));
            }
        }
        for neighbors in adjacency.values_mut() {
            neighbors.sort_by_key(|(doc_id, _)| *doc_id);
        }
        let edge_count = adjacency.values().map(Vec::len).sum();
        Self {
            adjacency,
            edge_count,
        }
    }

    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// Personalized PageRank over the small neighborhood reachable from lexical
    /// seeds. This is bounded by a fixed iteration count and does no vault scan.
    pub fn spread(&self, seeds: &[(DocId, f64)], limit: usize) -> HashMap<DocId, f64> {
        if seeds.is_empty() || limit == 0 || self.adjacency.is_empty() {
            return HashMap::new();
        }

        let mut restart = HashMap::<DocId, f64>::new();
        for (doc_id, score) in seeds {
            if *score > 0.0 {
                restart
                    .entry(*doc_id)
                    .and_modify(|current| *current = current.max(*score))
                    .or_insert(*score);
            }
        }
        normalize_mass(&mut restart);
        let mut current = restart.clone();

        for _ in 0..WALK_STEPS {
            let mut next = restart
                .iter()
                .map(|(doc_id, mass)| (*doc_id, mass * RESTART_PROBABILITY))
                .collect::<HashMap<_, _>>();
            for (source, mass) in &current {
                let Some(neighbors) = self.adjacency.get(source) else {
                    *next.entry(*source).or_default() += mass * (1.0 - RESTART_PROBABILITY);
                    continue;
                };
                let total_weight = neighbors.iter().map(|(_, weight)| *weight).sum::<f64>();
                if total_weight <= f64::EPSILON {
                    continue;
                }
                for (target, weight) in neighbors {
                    *next.entry(*target).or_default() +=
                        mass * (1.0 - RESTART_PROBABILITY) * weight / total_weight;
                }
            }
            current = next;
        }

        let max_score = current.values().copied().fold(0.0, f64::max);
        let mut ranked = current.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(&right.0))
        });
        ranked.truncate(limit);
        ranked
            .into_iter()
            .map(|(doc_id, score)| {
                let normalized = if max_score <= f64::EPSILON {
                    0.0
                } else {
                    score / max_score
                };
                (doc_id, normalized)
            })
            .collect()
    }
}

fn normalize_mass(values: &mut HashMap<DocId, f64>) {
    let total = values.values().sum::<f64>();
    if total > f64::EPSILON {
        for value in values.values_mut() {
            *value /= total;
        }
    }
}

fn resolve_target(
    source_path: &str,
    target: &str,
    exact: &HashMap<String, DocId>,
    stems: &HashMap<String, Vec<DocId>>,
) -> Option<DocId> {
    let canonical_target = canonical_path(target);
    if let Some(doc_id) = exact.get(&canonical_target) {
        return Some(*doc_id);
    }

    if let Some(parent) = Path::new(source_path).parent() {
        let relative = canonical_path(&parent.join(target).to_string_lossy());
        if let Some(doc_id) = exact.get(&relative) {
            return Some(*doc_id);
        }
    }

    let mut suffix_matches = exact
        .iter()
        .filter(|(path, _)| path.ends_with(&format!("/{canonical_target}")))
        .map(|(_, doc_id)| *doc_id);
    let suffix_match = suffix_matches.next();
    if suffix_match.is_some() && suffix_matches.next().is_none() {
        return suffix_match;
    }

    let stem = path_stem(target);
    stems
        .get(&stem)
        .filter(|doc_ids| doc_ids.len() == 1)
        .and_then(|doc_ids| doc_ids.first().copied())
}

fn canonical_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/").trim().to_lowercase();
    if normalized.ends_with(".md") {
        normalized.truncate(normalized.len() - 3);
    }
    normalized.trim_matches('/').to_string()
}

fn path_stem(path: &str) -> String {
    let value = Path::new(path)
        .file_stem()
        .or_else(|| Path::new(path).file_name())
        .map(|value| ascii_fold(&value.to_string_lossy()))
        .unwrap_or_else(|| ascii_fold(path));
    value
        .replace(['-', '_'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(doc_id: DocId, path: &str, content: &str) -> StoredDocument {
        StoredDocument {
            doc_id,
            source_path: path.to_string(),
            canonical_text: content.to_string(),
            title: None,
        }
    }

    #[test]
    fn graph_resolves_unique_wikilinks_and_backlinks() {
        let graph = DocumentGraph::build(&[
            document(1, "/vault/projects/launch.md", "See [[Legal Brief]]."),
            document(2, "/vault/reference/legal-brief.md", "Evidence"),
        ]);

        assert_eq!(graph.edge_count(), 2);
        let spread = graph.spread(&[(1, 1.0)], 10);
        assert!(spread.get(&2).copied().unwrap_or(0.0) > 0.0);
    }

    #[test]
    fn ambiguous_stems_do_not_create_false_edges() {
        let graph = DocumentGraph::build(&[
            document(1, "/vault/source.md", "See [[Index]]."),
            document(2, "/vault/a/index.md", "A"),
            document(3, "/vault/b/index.md", "B"),
        ]);

        assert_eq!(graph.edge_count(), 0);
    }
}
