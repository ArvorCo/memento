use anyhow::{Context, Result};
use libmemento::sync::discovery::discover_documents;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

mod baseline;
use baseline::BaselineIndex;
mod runtime;
use runtime::{ensure_daemon, post};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkCase {
    id: String,
    query: String,
    expected_path: String,
    expected_title: String,
    expected_terms: Vec<String>,
    excerpt: String,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    engine_version: &'static str,
    platform: String,
    dataset: String,
    dataset_sha256: String,
    corpus: CorpusSummary,
    cases: usize,
    top_k: usize,
    warmup_rounds: usize,
    repetitions: usize,
    memento: BenchmarkSummary,
    bm25f: BenchmarkSummary,
    comparison: BenchmarkComparison,
    per_case: Vec<BenchmarkCaseReport>,
}

#[derive(Debug, Serialize)]
struct CorpusSummary {
    root: String,
    discovered_documents: usize,
    indexed_documents: usize,
    bytes: u64,
    sha256: String,
    discovery_ms: f64,
    baseline_index_build_ms: f64,
}

#[derive(Debug, Serialize)]
struct BenchmarkSummary {
    hits: usize,
    hit_rate: f64,
    mrr: f64,
    avg_answer_term_recall: Option<f64>,
    avg_result_term_recall: f64,
    avg_confidence: Option<f64>,
    latency_ms: LatencySummary,
    misses: Vec<BenchmarkMiss>,
}

#[derive(Debug, Serialize)]
struct LatencySummary {
    total: f64,
    average: f64,
    p50: f64,
    p95: f64,
    p99: f64,
}

#[derive(Debug, Serialize)]
struct BenchmarkCaseReport {
    id: String,
    memento_rank: Option<usize>,
    bm25f_rank: Option<usize>,
    memento_top_path: Option<String>,
    bm25f_top_path: Option<String>,
    memento_latency_ms: LatencySummary,
    bm25f_latency_ms: LatencySummary,
    memento_confidence: f64,
    memento_rank_stable: bool,
    bm25f_rank_stable: bool,
}

#[derive(Debug, Serialize)]
struct BenchmarkComparison {
    hit_rate_delta: f64,
    mrr_delta: f64,
    result_term_recall_delta: f64,
    memento_only_hits: usize,
    bm25f_only_hits: usize,
}

#[derive(Debug, Serialize)]
struct BenchmarkMiss {
    id: String,
    query: String,
    expected_path: String,
    top_result_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QueryResponse {
    answer: String,
    results: Vec<QueryResult>,
    confidence: f64,
}

#[derive(Debug, Deserialize)]
struct QueryResult {
    content: String,
    source_path: String,
}

pub fn build_benchmark(vault: &str, output: &str, limit: usize) -> Result<()> {
    let vault_path = fs::canonicalize(expand_tilde(vault))
        .with_context(|| format!("Could not resolve vault path {}", vault))?;
    let output_path = PathBuf::from(output);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let documents = discover_documents(&vault_path)?;
    let files: Vec<PathBuf> = documents
        .into_iter()
        .map(|document| document.path)
        .filter(|path| is_markdown(path))
        .collect();
    let files = evenly_sample(files, limit);

    let mut cases = Vec::new();
    for path in files {
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let title = extract_title(&path, &contents);
        let excerpt = extract_excerpt(&contents);
        let expected_terms = derive_keywords(&title, &excerpt);
        if expected_terms.is_empty() {
            continue;
        }

        let query = build_query(&title, &expected_terms);
        let relative = path
            .strip_prefix(&vault_path)
            .unwrap_or(path.as_path())
            .display()
            .to_string();

        cases.push(BenchmarkCase {
            id: relative.replace('/', "::"),
            query,
            expected_path: relative,
            expected_title: title,
            expected_terms,
            excerpt,
        });
    }

    let mut jsonl = String::new();
    for case in &cases {
        jsonl.push_str(&serde_json::to_string(case)?);
        jsonl.push('\n');
    }
    fs::write(&output_path, jsonl)?;

    println!("benchmark dataset: {}", output_path.display());
    println!("cases: {}", cases.len());
    println!("vault: {}", vault_path.display());

    Ok(())
}

fn evenly_sample<T>(values: Vec<T>, limit: usize) -> Vec<T> {
    if limit == 0 {
        return Vec::new();
    }
    if values.len() <= limit {
        return values;
    }
    let mut values: Vec<Option<T>> = values.into_iter().map(Some).collect();
    let last = values.len() - 1;
    (0..limit)
        .map(|sample| {
            let index = sample * last / (limit - 1).max(1);
            values[index].take().expect("sample indexes are unique")
        })
        .collect()
}

pub async fn run_benchmark(
    dataset: &str,
    corpus: &str,
    top_k: usize,
    limit: Option<usize>,
    warmup_rounds: usize,
    repetitions: usize,
    report: &str,
) -> Result<()> {
    anyhow::ensure!(top_k > 0, "--top-k must be greater than zero");
    anyhow::ensure!(repetitions > 0, "--repetitions must be greater than zero");

    let dataset_path = fs::canonicalize(expand_tilde(dataset))
        .with_context(|| format!("Could not resolve dataset path {dataset}"))?;
    let raw = fs::read_to_string(&dataset_path)
        .with_context(|| format!("Could not read dataset {}", dataset_path.display()))?;
    let mut cases = Vec::new();
    for (line_number, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let case: BenchmarkCase = serde_json::from_str(line).with_context(|| {
            format!(
                "Invalid benchmark case at {}:{}",
                dataset_path.display(),
                line_number + 1
            )
        })?;
        cases.push(case);
    }
    if let Some(limit) = limit {
        cases.truncate(limit);
    }
    anyhow::ensure!(
        !cases.is_empty(),
        "Benchmark dataset does not contain any cases"
    );

    let corpus_started = Instant::now();
    let corpus_path = fs::canonicalize(expand_tilde(corpus))
        .with_context(|| format!("Could not resolve corpus path {corpus}"))?;
    anyhow::ensure!(corpus_path.is_dir(), "Corpus must be a directory");
    let discovered = discover_documents(&corpus_path)?;
    let discovery_ms = corpus_started.elapsed().as_secs_f64() * 1_000.0;
    anyhow::ensure!(
        !discovered.is_empty(),
        "Corpus does not contain any supported non-empty documents"
    );

    let corpus_paths = discovered
        .iter()
        .map(|document| canonicalize_loose(&document.path.to_string_lossy()))
        .collect::<std::collections::HashSet<_>>();
    let mut case_ids = std::collections::HashSet::new();
    for case in &mut cases {
        anyhow::ensure!(
            case_ids.insert(case.id.clone()),
            "Duplicate benchmark case id `{}`",
            case.id
        );
        let expected_path = PathBuf::from(&case.expected_path);
        case.expected_path = if expected_path.is_absolute() {
            canonicalize_loose(&case.expected_path)
        } else {
            canonicalize_loose(&corpus_path.join(expected_path).to_string_lossy())
        };
        anyhow::ensure!(
            corpus_paths.contains(&case.expected_path),
            "Expected path for case `{}` is not in the explicit corpus: {}",
            case.id,
            case.expected_path
        );
    }

    let (corpus_sha256, corpus_bytes) = corpus_fingerprint(&corpus_path, &discovered)?;
    let bm25f_index = BaselineIndex::build(&discovered)?;
    anyhow::ensure!(
        bm25f_index.document_count() > 0,
        "No corpus documents could be parsed for the BM25F baseline"
    );

    ensure_daemon().await?;
    for _ in 0..warmup_rounds {
        for case in &cases {
            let body = serde_json::json!({ "query": case.query, "top_k": top_k });
            let response = post("/query", &body.to_string()).await?;
            let _: QueryResponse = serde_json::from_str(&response)?;
            let _ = bm25f_index.search(&case.query, top_k);
        }
    }

    let mut memento_hits = 0usize;
    let mut memento_reciprocal_rank_sum = 0.0;
    let mut memento_answer_term_sum = 0.0;
    let mut memento_result_term_sum = 0.0;
    let mut memento_confidence_sum = 0.0;
    let mut memento_misses = Vec::new();

    let mut bm25f_hits = 0usize;
    let mut bm25f_reciprocal_rank_sum = 0.0;
    let mut bm25f_result_term_sum = 0.0;
    let mut bm25f_misses = Vec::new();

    let mut memento_only_hits = 0usize;
    let mut bm25f_only_hits = 0usize;
    let observation_count = cases.len() * repetitions;
    let mut memento_latencies = Vec::with_capacity(observation_count);
    let mut bm25f_latencies = Vec::with_capacity(observation_count);
    let mut per_case = Vec::with_capacity(cases.len());

    for case in &cases {
        let body = serde_json::json!({
            "query": case.query,
            "top_k": top_k,
        });
        let mut case_memento_latencies = Vec::with_capacity(repetitions);
        let mut case_bm25f_latencies = Vec::with_capacity(repetitions);
        let mut memento_ranks = Vec::with_capacity(repetitions);
        let mut bm25f_ranks = Vec::with_capacity(repetitions);
        let mut parsed_response = None;
        let mut first_bm25f_paths = None;

        for _ in 0..repetitions {
            let memento_started = Instant::now();
            let response = post("/query", &body.to_string()).await?;
            let memento_latency_ms = memento_started.elapsed().as_secs_f64() * 1_000.0;
            memento_latencies.push(memento_latency_ms);
            case_memento_latencies.push(memento_latency_ms);
            let parsed: QueryResponse = serde_json::from_str(&response)?;
            memento_ranks.push(
                parsed.results.iter().position(|result| {
                    canonicalize_loose(&result.source_path) == case.expected_path
                }),
            );
            if parsed_response.is_none() {
                parsed_response = Some(parsed);
            }

            let bm25f_started = Instant::now();
            let results = bm25f_index.search(&case.query, top_k);
            let bm25f_latency_ms = bm25f_started.elapsed().as_secs_f64() * 1_000.0;
            bm25f_latencies.push(bm25f_latency_ms);
            case_bm25f_latencies.push(bm25f_latency_ms);
            bm25f_ranks.push(
                results
                    .iter()
                    .position(|result| result.path == case.expected_path),
            );
            if first_bm25f_paths.is_none() {
                first_bm25f_paths = Some(
                    results
                        .into_iter()
                        .map(|result| (result.path.to_string(), result.content.to_string()))
                        .collect::<Vec<_>>(),
                );
            }
        }

        let parsed = parsed_response.expect("repetitions are non-zero");
        let bm25f_results = first_bm25f_paths.expect("repetitions are non-zero");
        let rank = memento_ranks[0];
        let bm25f_rank = bm25f_ranks[0];

        let memento_hit = rank.is_some();
        if let Some(rank) = rank {
            memento_hits += 1;
            memento_reciprocal_rank_sum += 1.0 / (rank as f64 + 1.0);
        } else {
            memento_misses.push(BenchmarkMiss {
                id: case.id.clone(),
                query: case.query.clone(),
                expected_path: case.expected_path.clone(),
                top_result_path: parsed
                    .results
                    .first()
                    .map(|result| result.source_path.clone()),
            });
        }

        memento_answer_term_sum += term_recall(&parsed.answer, &case.expected_terms);
        memento_confidence_sum += parsed.confidence;
        let combined_results = parsed
            .results
            .iter()
            .map(|result| result.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        memento_result_term_sum += term_recall(&combined_results, &case.expected_terms);

        let bm25f_hit = bm25f_rank.is_some();
        if let Some(rank) = bm25f_rank {
            bm25f_hits += 1;
            bm25f_reciprocal_rank_sum += 1.0 / (rank as f64 + 1.0);
        } else {
            bm25f_misses.push(BenchmarkMiss {
                id: case.id.clone(),
                query: case.query.clone(),
                expected_path: case.expected_path.clone(),
                top_result_path: bm25f_results.first().map(|result| result.0.clone()),
            });
        }
        let bm25f_combined = bm25f_results
            .iter()
            .map(|result| result.1.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        bm25f_result_term_sum += term_recall(&bm25f_combined, &case.expected_terms);

        if memento_hit && !bm25f_hit {
            memento_only_hits += 1;
        } else if bm25f_hit && !memento_hit {
            bm25f_only_hits += 1;
        }

        per_case.push(BenchmarkCaseReport {
            id: case.id.clone(),
            memento_rank: rank.map(|value| value + 1),
            bm25f_rank: bm25f_rank.map(|value| value + 1),
            memento_top_path: parsed
                .results
                .first()
                .map(|result| result.source_path.clone()),
            bm25f_top_path: bm25f_results.first().map(|result| result.0.clone()),
            memento_latency_ms: summarize_latencies(&case_memento_latencies),
            bm25f_latency_ms: summarize_latencies(&case_bm25f_latencies),
            memento_confidence: parsed.confidence,
            memento_rank_stable: ranks_are_stable(&memento_ranks),
            bm25f_rank_stable: ranks_are_stable(&bm25f_ranks),
        });
    }

    let case_count = cases.len();
    let memento = BenchmarkSummary {
        hits: memento_hits,
        hit_rate: memento_hits as f64 / case_count as f64,
        mrr: memento_reciprocal_rank_sum / case_count as f64,
        avg_answer_term_recall: Some(memento_answer_term_sum / case_count as f64),
        avg_result_term_recall: memento_result_term_sum / case_count as f64,
        avg_confidence: Some(memento_confidence_sum / case_count as f64),
        latency_ms: summarize_latencies(&memento_latencies),
        misses: memento_misses.into_iter().take(12).collect(),
    };
    let bm25f = BenchmarkSummary {
        hits: bm25f_hits,
        hit_rate: bm25f_hits as f64 / case_count as f64,
        mrr: bm25f_reciprocal_rank_sum / case_count as f64,
        avg_answer_term_recall: None,
        avg_result_term_recall: bm25f_result_term_sum / case_count as f64,
        avg_confidence: None,
        latency_ms: summarize_latencies(&bm25f_latencies),
        misses: bm25f_misses.into_iter().take(12).collect(),
    };
    let report_data = BenchmarkReport {
        schema_version: 2,
        engine_version: env!("CARGO_PKG_VERSION"),
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        dataset: dataset_path.display().to_string(),
        dataset_sha256: sha256_bytes(raw.as_bytes()),
        corpus: CorpusSummary {
            root: corpus_path.display().to_string(),
            discovered_documents: discovered.len(),
            indexed_documents: bm25f_index.document_count(),
            bytes: corpus_bytes,
            sha256: corpus_sha256,
            discovery_ms,
            baseline_index_build_ms: bm25f_index.build_ms,
        },
        cases: cases.len(),
        top_k,
        warmup_rounds,
        repetitions,
        comparison: BenchmarkComparison {
            hit_rate_delta: memento.hit_rate - bm25f.hit_rate,
            mrr_delta: memento.mrr - bm25f.mrr,
            result_term_recall_delta: memento.avg_result_term_recall - bm25f.avg_result_term_recall,
            memento_only_hits,
            bm25f_only_hits,
        },
        memento,
        bm25f,
        per_case,
    };

    let report_path = PathBuf::from(report);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&report_path, serde_json::to_string_pretty(&report_data)?)?;

    println!("benchmark report: {}", report_path.display());
    println!("cases: {}", report_data.cases);
    println!(
        "corpus: {} documents, {} bytes, sha256 {}",
        report_data.corpus.indexed_documents, report_data.corpus.bytes, report_data.corpus.sha256
    );
    println!(
        "memento hit@{}: {:.1}%",
        top_k,
        report_data.memento.hit_rate * 100.0
    );
    println!("memento mrr: {:.3}", report_data.memento.mrr);
    println!(
        "memento latency: avg {:.1} ms, p50 {:.1} ms, p95 {:.1} ms",
        report_data.memento.latency_ms.average,
        report_data.memento.latency_ms.p50,
        report_data.memento.latency_ms.p95
    );
    println!(
        "memento answer term recall: {:.3}",
        report_data
            .memento
            .avg_answer_term_recall
            .unwrap_or_default()
    );
    println!(
        "memento result term recall: {:.3}",
        report_data.memento.avg_result_term_recall
    );
    println!(
        "bm25f hit@{}: {:.1}%",
        top_k,
        report_data.bm25f.hit_rate * 100.0
    );
    println!("bm25f mrr: {:.3}", report_data.bm25f.mrr);
    println!(
        "bm25f latency: avg {:.1} ms, p50 {:.1} ms, p95 {:.1} ms",
        report_data.bm25f.latency_ms.average,
        report_data.bm25f.latency_ms.p50,
        report_data.bm25f.latency_ms.p95
    );
    println!(
        "bm25f result term recall: {:.3}",
        report_data.bm25f.avg_result_term_recall
    );
    println!(
        "delta hit@{}: {:+.1}%",
        top_k,
        report_data.comparison.hit_rate_delta * 100.0
    );
    println!("delta mrr: {:+.3}", report_data.comparison.mrr_delta);

    Ok(())
}

fn ranks_are_stable(ranks: &[Option<usize>]) -> bool {
    ranks
        .first()
        .is_none_or(|first| ranks.iter().all(|rank| rank == first))
}

fn corpus_fingerprint(
    root: &Path,
    documents: &[libmemento::sync::discovery::DiscoveredDocument],
) -> Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let mut total_bytes = 0_u64;
    for document in documents {
        let relative = document.path.strip_prefix(root).unwrap_or(&document.path);
        let relative = relative.to_string_lossy().replace('\\', "/");
        let contents = fs::read(&document.path)
            .with_context(|| format!("Could not fingerprint {}", document.path.display()))?;
        total_bytes = total_bytes.saturating_add(contents.len() as u64);
        hasher.update((relative.len() as u64).to_le_bytes());
        hasher.update(relative.as_bytes());
        hasher.update((contents.len() as u64).to_le_bytes());
        hasher.update(&contents);
    }
    Ok((hex_digest(hasher.finalize()), total_bytes))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_digest(hasher.finalize())
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn summarize_latencies(values: &[f64]) -> LatencySummary {
    if values.is_empty() {
        return LatencySummary {
            total: 0.0,
            average: 0.0,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
        };
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let total = sorted.iter().sum::<f64>();
    LatencySummary {
        total,
        average: total / sorted.len() as f64,
        p50: percentile(&sorted, 0.50),
        p95: percentile(&sorted, 0.95),
        p99: percentile(&sorted, 0.99),
    }
}

fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * quantile).ceil() as usize;
    sorted[index]
}

fn extract_title(path: &Path, contents: &str) -> String {
    contents
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("untitled")
                .replace(['-', '_'], " ")
        })
}

fn extract_excerpt(contents: &str) -> String {
    let mut buffer = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("```") {
            continue;
        }
        buffer.push(line);
        if buffer.join(" ").len() > 180 {
            break;
        }
    }
    buffer.join(" ").chars().take(220).collect()
}

fn derive_keywords(title: &str, excerpt: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for text in [title, excerpt] {
        for token in tokenize(text) {
            if token.len() >= 4
                && !token.chars().all(|char| char.is_ascii_digit())
                && !is_generic_term(&token)
                && !terms.contains(&token)
            {
                terms.push(token);
            }
            if terms.len() == 6 {
                return terms;
            }
        }
    }
    terms
}

fn build_query(title: &str, expected_terms: &[String]) -> String {
    let lowered = title.trim().to_lowercase();
    if lowered.starts_with("how ")
        || lowered.starts_with("why ")
        || lowered.starts_with("what ")
        || lowered.starts_with("when ")
        || lowered.starts_with("where ")
    {
        title.trim().to_string()
    } else if looks_like_journal_title(&lowered) {
        let focus_terms: Vec<&str> = expected_terms
            .iter()
            .filter(|term| !is_generic_term(term))
            .take(3)
            .map(|term| term.as_str())
            .collect();
        if focus_terms.is_empty() {
            format!("what did we record in {}?", title.trim())
        } else {
            format!("what did we record about {}?", focus_terms.join(", "))
        }
    } else {
        format!("what does {} say?", title.trim())
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.nfd()
        .filter(|character| !is_combining_mark(*character))
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn looks_like_journal_title(title: &str) -> bool {
    title.chars().take(4).all(|char| char.is_ascii_digit())
        || [
            "daily", "session", "notes", "report", "review", "planning", "log",
        ]
        .iter()
        .any(|term| title.contains(term))
}

fn is_generic_term(term: &str) -> bool {
    matches!(
        term,
        "daily"
            | "report"
            | "review"
            | "planning"
            | "session"
            | "notes"
            | "memory"
            | "journal"
            | "today"
            | "yesterday"
            | "agent"
            | "agents"
            | "first"
            | "second"
            | "third"
            | "spent"
            | "about"
            | "what"
            | "when"
            | "where"
            | "which"
            | "who"
            | "why"
            | "how"
            | "does"
            | "did"
            | "say"
            | "said"
            | "with"
            | "from"
            | "that"
            | "this"
            | "there"
            | "recorded"
            | "qual"
            | "quais"
            | "quem"
            | "quando"
            | "onde"
            | "como"
            | "porque"
            | "sobre"
            | "disse"
            | "esta"
            | "esse"
            | "essa"
            | "aquela"
            | "foi"
            | "tem"
            | "para"
            | "pela"
            | "pelo"
            | "das"
            | "dos"
            | "uma"
            | "com"
    )
}

fn term_recall(text: &str, expected_terms: &[String]) -> f64 {
    if expected_terms.is_empty() {
        return 0.0;
    }
    let haystack = normalize_recall_text(text);
    let compact_haystack = haystack
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect::<String>();
    let hits = expected_terms
        .iter()
        .filter(|term| {
            let needle = normalize_recall_text(term);
            haystack.contains(&needle)
                || (needle
                    .chars()
                    .all(|character| character.is_ascii_digit() || character.is_whitespace())
                    && compact_haystack.contains(
                        &needle
                            .chars()
                            .filter(|character| character.is_ascii_digit())
                            .collect::<String>(),
                    ))
        })
        .count();
    hits as f64 / expected_terms.len() as f64
}

fn normalize_recall_text(text: &str) -> String {
    let normalized = text
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>();
    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("md" | "markdown")
    )
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/").or_else(|| path.strip_prefix(r"~\")) {
        return dirs::home_dir()
            .expect("Could not determine home directory")
            .join(stripped);
    }
    PathBuf::from(path)
}

fn canonicalize_loose(path: &str) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        build_query, derive_keywords, extract_excerpt, extract_title, looks_like_journal_title,
        runtime::process_is_alive, summarize_latencies, term_recall,
    };
    use std::path::Path;

    #[test]
    fn title_uses_heading_first() {
        let title = extract_title(Path::new("/tmp/example.md"), "# Roadmap\nBody");
        assert_eq!(title, "Roadmap");
    }

    #[test]
    fn excerpt_skips_heading_lines() {
        let excerpt = extract_excerpt("# Title\n\nfirst line\nsecond line");
        assert!(excerpt.contains("first line"));
        assert!(!excerpt.contains("# Title"));
    }

    #[test]
    fn current_process_is_reported_alive() {
        assert!(process_is_alive(std::process::id() as i32));
    }

    #[test]
    fn query_wraps_non_question_titles() {
        assert_eq!(
            build_query("Auth Decisions", &["auth".into(), "decisions".into()]),
            "what does Auth Decisions say?"
        );
    }

    #[test]
    fn keywords_include_title_terms() {
        let keywords = derive_keywords("Authentication Decisions", "Token refresh rotation");
        assert!(keywords.contains(&"authentication".to_string()));
        assert!(keywords.contains(&"decisions".to_string()));
    }

    #[test]
    fn journal_titles_switch_to_keyword_queries() {
        assert!(looks_like_journal_title("2026-01-24 daily review"));
        assert_eq!(
            build_query(
                "2026-01-24 Daily Review",
                &["daily".into(), "authentication".into(), "rotation".into()]
            ),
            "what did we record about authentication, rotation?"
        );
    }

    #[test]
    fn latency_summary_reports_tail_percentiles() {
        let summary = summarize_latencies(&[1.0, 2.0, 3.0, 20.0]);
        assert_eq!(summary.total, 26.0);
        assert_eq!(summary.average, 6.5);
        assert_eq!(summary.p50, 3.0);
        assert_eq!(summary.p95, 20.0);
        assert_eq!(summary.p99, 20.0);
    }

    #[test]
    fn term_recall_normalizes_accents_and_number_separators() {
        let expected = vec!["itau".to_string(), "1093".to_string()];

        assert_eq!(term_recall("Itaú converted 1,093 leads", &expected), 1.0);
    }
}
