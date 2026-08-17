use crate::text_utils::{
    ascii_fold, parse_date_tokens, query_exactness_terms, query_has_any_term, strip_date_prefix,
    tokenize_folded_text,
};
use libmemento::format::{DocId, StoredChunk, StoredDocument};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryClass {
    DailyNote,
    Review,
    Research,
    Summary,
    Guide,
    ProjectNote,
    Derived,
    RetrievalLog,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationRule {
    pub name: String,
    pub class: String,
    #[serde(default)]
    pub path_contains: Vec<String>,
    #[serde(default)]
    pub title_contains: Vec<String>,
    #[serde(default)]
    pub tag_contains: Vec<String>,
    #[serde(default)]
    pub content_contains: Vec<String>,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationRulesConfig {
    #[serde(default)]
    pub rules: Vec<ClassificationRule>,
}

pub fn default_classification_rules() -> ClassificationRulesConfig {
    ClassificationRulesConfig {
        rules: vec![
            ClassificationRule {
                name: "review notes".to_string(),
                class: "review".to_string(),
                path_contains: vec![
                    "review".to_string(),
                    "retro".to_string(),
                    "checkin".to_string(),
                ],
                title_contains: vec!["review".to_string(), "retro".to_string()],
                tag_contains: vec!["review".to_string()],
                content_contains: Vec::new(),
                priority: 90,
            },
            ClassificationRule {
                name: "research notes".to_string(),
                class: "research".to_string(),
                path_contains: vec![
                    "research".to_string(),
                    "inspiration".to_string(),
                    "insights".to_string(),
                    "analysis".to_string(),
                    "article".to_string(),
                ],
                title_contains: vec![
                    "research".to_string(),
                    "inspiration".to_string(),
                    "insights".to_string(),
                    "analysis".to_string(),
                    "article".to_string(),
                ],
                tag_contains: vec!["research".to_string()],
                content_contains: Vec::new(),
                priority: 80,
            },
            ClassificationRule {
                name: "summary notes".to_string(),
                class: "summary".to_string(),
                path_contains: vec![
                    "summary".to_string(),
                    "overview".to_string(),
                    "digest".to_string(),
                    "intel".to_string(),
                    "weekly".to_string(),
                    "recap".to_string(),
                ],
                title_contains: vec![
                    "summary".to_string(),
                    "overview".to_string(),
                    "digest".to_string(),
                    "intel".to_string(),
                    "weekly".to_string(),
                    "recap".to_string(),
                ],
                tag_contains: vec!["summary".to_string()],
                content_contains: Vec::new(),
                priority: 70,
            },
            ClassificationRule {
                name: "project notes".to_string(),
                class: "project_note".to_string(),
                path_contains: vec![
                    "gtm".to_string(),
                    "notion".to_string(),
                    "wacli".to_string(),
                    "arvor".to_string(),
                    "supabase".to_string(),
                    "campaign".to_string(),
                    "growth".to_string(),
                    "auth".to_string(),
                    "project".to_string(),
                ],
                title_contains: vec![
                    "gtm".to_string(),
                    "notion".to_string(),
                    "wacli".to_string(),
                    "arvor".to_string(),
                    "supabase".to_string(),
                    "campaign".to_string(),
                    "growth".to_string(),
                    "auth".to_string(),
                    "project".to_string(),
                ],
                tag_contains: vec!["project".to_string(), "area/project".to_string()],
                content_contains: Vec::new(),
                priority: 75,
            },
            ClassificationRule {
                name: "guides".to_string(),
                class: "guide".to_string(),
                path_contains: vec![
                    "guide".to_string(),
                    "protocol".to_string(),
                    "playbook".to_string(),
                    "blueprint".to_string(),
                    "template".to_string(),
                ],
                title_contains: vec![
                    "guide".to_string(),
                    "protocol".to_string(),
                    "playbook".to_string(),
                    "blueprint".to_string(),
                    "template".to_string(),
                ],
                tag_contains: vec!["guide".to_string()],
                content_contains: Vec::new(),
                priority: 60,
            },
        ],
    }
}

pub fn classification_rules_path(data_dir: &Path) -> PathBuf {
    data_dir.join("config").join("classification_rules.json")
}

pub fn load_or_bootstrap_classification_rules(
    data_dir: &Path,
) -> anyhow::Result<ClassificationRulesConfig> {
    let path = classification_rules_path(data_dir);
    if path.exists() {
        let payload = fs::read_to_string(&path)?;
        let config = serde_json::from_str(&payload)?;
        return Ok(config);
    }

    let config = default_classification_rules();
    let payload = serde_json::to_vec_pretty(&config)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, payload)?;
    Ok(config)
}

pub(crate) fn folded_document_content(
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> String {
    if let Some(document) = documents.get(&chunk.doc_id) {
        if !document.canonical_text.is_empty() {
            return ascii_fold(&document.canonical_text);
        }
    }
    ascii_fold(chunk.resolve_content(documents))
}

pub(crate) fn source_profile(
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> (String, Option<(u32, u32, u32)>) {
    let mut profile = chunk.source_path.clone();
    if let Some(section_title) = &chunk.section_title {
        profile.push(' ');
        profile.push_str(section_title);
    }
    if let Some(document) = documents.get(&chunk.doc_id) {
        if let Some(title) = &document.title {
            profile.push(' ');
            profile.push_str(title);
        }
    }
    let date = parse_date_tokens(&profile);
    (profile.to_lowercase(), date)
}

pub fn exact_metadata_text(
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> String {
    let mut text = chunk.source_path.clone();
    if let Some(document) = documents.get(&chunk.doc_id) {
        if let Some(title) = &document.title {
            text.push(' ');
            text.push_str(title);
        }
    }
    if let Some(section_title) = &chunk.section_title {
        text.push(' ');
        text.push_str(section_title);
    }
    ascii_fold(&text)
}

fn entity_aliases(chunk: &StoredChunk, documents: &HashMap<DocId, &StoredDocument>) -> Vec<String> {
    let mut aliases = Vec::new();

    if let Some(document) = documents.get(&chunk.doc_id) {
        if let Some(title) = &document.title {
            aliases.push(ascii_fold(title));
        }
    }

    let path = Path::new(&chunk.source_path);
    if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
        let folded = ascii_fold(stem);
        aliases.push(folded.clone());
        aliases.push(ascii_fold(&strip_date_prefix(stem)));
    }

    aliases.sort_unstable();
    aliases.dedup();
    aliases.retain(|alias| !alias.trim().is_empty());
    aliases
}

pub fn entity_lookup_score(
    query: &str,
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> f64 {
    let folded_query = ascii_fold(query);
    let query_terms = query_exactness_terms(query);
    if query_terms.is_empty() {
        return 0.0;
    }

    let aliases = entity_aliases(chunk, documents);
    if aliases.is_empty() {
        return 0.0;
    }

    let mut best: f64 = 0.0;
    for alias in aliases {
        if alias == folded_query.trim() || folded_query.contains(alias.as_str()) {
            best = best.max(1.0);
            continue;
        }
        let alias_terms = tokenize_folded_text(&alias);
        if alias_terms.is_empty() {
            continue;
        }
        let matched = query_terms
            .iter()
            .filter(|term| alias_terms.contains(term) || alias.contains(term.as_str()))
            .count();
        let coverage = matched as f64 / query_terms.len() as f64;
        let phrase_bonus = if query_terms.len() >= 2
            && query_terms
                .windows(2)
                .any(|window| alias.contains(&window.join(" ")))
        {
            0.2
        } else {
            0.0
        };
        best = best.max((coverage + phrase_bonus).clamp(0.0, 1.0));
    }

    best
}

fn frontmatter_tags(document: &StoredDocument) -> Vec<String> {
    let mut tags = Vec::new();
    let text = document.canonical_text.trim_start();
    if !text.starts_with("---\n") {
        return tags;
    }

    let mut lines = text.lines();
    let _ = lines.next();
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        let folded = ascii_fold(trimmed);
        if let Some(rest) = folded.strip_prefix("tags:") {
            tags.extend(
                rest.split([',', '[', ']', ' '])
                    .filter(|part| !part.is_empty())
                    .map(|part| part.trim_matches('"').trim_matches('\'').to_string()),
            );
        }
    }

    tags.sort_unstable();
    tags.dedup();
    tags
}

fn rule_class(rule_class: &str) -> MemoryClass {
    match rule_class {
        "daily_note" => MemoryClass::DailyNote,
        "review" => MemoryClass::Review,
        "research" => MemoryClass::Research,
        "summary" => MemoryClass::Summary,
        "guide" => MemoryClass::Guide,
        "project_note" => MemoryClass::ProjectNote,
        _ => MemoryClass::Other,
    }
}

fn classify_memory_by_rules(
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
    rules: &ClassificationRulesConfig,
) -> Option<MemoryClass> {
    let folded_path = ascii_fold(&chunk.source_path);
    let folded_title = documents
        .get(&chunk.doc_id)
        .and_then(|document| document.title.as_ref())
        .map(|title| ascii_fold(title))
        .unwrap_or_default();
    let needs_content = rules
        .rules
        .iter()
        .any(|rule| !rule.content_contains.is_empty());
    let folded_content = if needs_content {
        documents
            .get(&chunk.doc_id)
            .map(|_| folded_document_content(chunk, documents))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let tags: Vec<String> = documents
        .get(&chunk.doc_id)
        .map(|document| frontmatter_tags(document))
        .unwrap_or_default();

    let mut matched = rules
        .rules
        .iter()
        .filter(|rule| {
            let path_match = !rule.path_contains.is_empty()
                && rule
                    .path_contains
                    .iter()
                    .any(|term| folded_path.contains(&ascii_fold(term)));
            let title_match = !rule.title_contains.is_empty()
                && rule
                    .title_contains
                    .iter()
                    .any(|term| folded_title.contains(&ascii_fold(term)));
            let tag_match = !rule.tag_contains.is_empty()
                && rule
                    .tag_contains
                    .iter()
                    .any(|term| tags.contains(&ascii_fold(term)));
            let content_match = !rule.content_contains.is_empty()
                && rule
                    .content_contains
                    .iter()
                    .any(|term| folded_content.contains(&ascii_fold(term)));

            path_match || title_match || tag_match || content_match
        })
        .collect::<Vec<_>>();

    matched.sort_by_key(|rule| std::cmp::Reverse(rule.priority));
    matched.first().map(|rule| rule_class(&rule.class))
}

pub fn classify_memory(
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
    rules: &ClassificationRulesConfig,
) -> MemoryClass {
    let normalized_path = chunk.source_path.replace('\\', "/").to_ascii_lowercase();
    if normalized_path.contains("/.dreams/events.jsonl") {
        return MemoryClass::RetrievalLog;
    }
    if normalized_path.contains("/.dreams/") || normalized_path.contains("/dreaming/") {
        return MemoryClass::Derived;
    }

    if let Some(class) = classify_memory_by_rules(chunk, documents, rules) {
        return class;
    }

    let (profile, date) = source_profile(chunk, documents);

    if [
        "guide",
        "protocol",
        "fundamentals",
        "strategy",
        "playbook",
        "template",
        "blueprint",
    ]
    .iter()
    .any(|term| profile.contains(term))
    {
        return MemoryClass::Guide;
    }

    if [
        "summary", "overview", "digest", "intel", "weekly", "launch", "recap",
    ]
    .iter()
    .any(|term| profile.contains(term))
    {
        return MemoryClass::Summary;
    }

    if ["review", "retro", "checkin", "check-in", "evening review"]
        .iter()
        .any(|term| profile.contains(term))
    {
        return MemoryClass::Review;
    }

    if [
        "research",
        "inspiration",
        "insights",
        "analysis",
        "study",
        "article",
    ]
    .iter()
    .any(|term| profile.contains(term))
    {
        return MemoryClass::Research;
    }

    if [
        "gtm", "notion", "wacli", "arvor", "supabase", "campaign", "growth", "auth",
    ]
    .iter()
    .any(|term| profile.contains(term))
    {
        return MemoryClass::ProjectNote;
    }

    if date.is_some() {
        return MemoryClass::DailyNote;
    }

    MemoryClass::Other
}

pub fn memory_class_score(query_terms: &[String], recall_intent: bool, class: MemoryClass) -> f64 {
    let prefers_review = query_has_any_term(query_terms, &["review", "evening", "retro"]);
    let prefers_research = query_has_any_term(
        query_terms,
        &[
            "research", "insights", "analysis", "article", "future", "study",
        ],
    );
    let prefers_summary = query_has_any_term(query_terms, &["summary", "overview", "weekly"]);
    let prefers_project = query_has_any_term(
        query_terms,
        &[
            "tracking", "migrado", "campaign", "launch", "supabase", "arvor", "wacli",
        ],
    );

    match class {
        MemoryClass::DailyNote => {
            if recall_intent && !prefers_review && !prefers_research {
                0.10
            } else {
                0.0
            }
        }
        MemoryClass::Review => {
            if prefers_review {
                0.14
            } else if recall_intent {
                0.02
            } else {
                0.0
            }
        }
        MemoryClass::Research => {
            if prefers_research {
                0.14
            } else if recall_intent {
                -0.08
            } else {
                0.0
            }
        }
        MemoryClass::Summary => {
            if prefers_summary {
                0.10
            } else if recall_intent {
                -0.10
            } else {
                0.0
            }
        }
        MemoryClass::Guide => {
            if recall_intent {
                -0.12
            } else {
                0.0
            }
        }
        MemoryClass::ProjectNote => {
            if prefers_project {
                0.10
            } else if recall_intent && !prefers_research {
                0.06
            } else {
                0.0
            }
        }
        MemoryClass::Derived => -0.18,
        MemoryClass::RetrievalLog => -0.60,
        MemoryClass::Other => 0.0,
    }
}
