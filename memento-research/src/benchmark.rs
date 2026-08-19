use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

#[cfg(windows)]
use tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
use tokio::net::UnixStream;

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
    dataset: String,
    cases: usize,
    top_k: usize,
    memento: BenchmarkSummary,
    simple_search: BenchmarkSummary,
    comparison: BenchmarkComparison,
    per_case: Vec<BenchmarkCaseReport>,
}

#[derive(Debug, Serialize)]
struct BenchmarkSummary {
    hits: usize,
    hit_rate: f64,
    mrr: f64,
    avg_answer_term_recall: f64,
    avg_result_term_recall: f64,
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
    simple_rank: Option<usize>,
    memento_top_path: Option<String>,
    simple_top_path: Option<String>,
    memento_latency_ms: f64,
    simple_latency_ms: f64,
}

#[derive(Debug, Serialize)]
struct BenchmarkComparison {
    hit_rate_delta: f64,
    mrr_delta: f64,
    answer_term_recall_delta: f64,
    result_term_recall_delta: f64,
    memento_only_hits: usize,
    simple_only_hits: usize,
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
}

#[derive(Debug, Deserialize)]
struct QueryResult {
    content: String,
    source_path: String,
}

#[derive(Debug, Clone)]
struct SimpleSearchDocument {
    path: String,
    title: String,
    title_tokens: Vec<String>,
    path_tokens: Vec<String>,
    content_tokens: Vec<String>,
    content: String,
}

pub fn build_benchmark(vault: &str, output: &str, limit: usize) -> Result<()> {
    let vault_path = fs::canonicalize(expand_tilde(vault))
        .with_context(|| format!("Could not resolve vault path {}", vault))?;
    let output_path = PathBuf::from(output);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut files: Vec<PathBuf> = WalkDir::new(&vault_path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| is_markdown(path))
        .collect();
    files.sort();

    let mut cases = Vec::new();
    for path in files.into_iter().take(limit) {
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
            expected_path: fs::canonicalize(&path)
                .unwrap_or(path.clone())
                .display()
                .to_string(),
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

pub async fn run_benchmark(
    dataset: &str,
    top_k: usize,
    limit: Option<usize>,
    report: &str,
) -> Result<()> {
    ensure_daemon().await?;

    let dataset_path = PathBuf::from(dataset);
    let raw = fs::read_to_string(&dataset_path)
        .with_context(|| format!("Could not read dataset {}", dataset_path.display()))?;
    let mut cases = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let case: BenchmarkCase = serde_json::from_str(line)?;
        cases.push(case);
    }
    if let Some(limit) = limit {
        cases.truncate(limit);
    }

    let simple_documents = load_simple_search_documents(&cases)?;

    let mut memento_hits = 0usize;
    let mut memento_reciprocal_rank_sum = 0.0;
    let mut memento_answer_term_sum = 0.0;
    let mut memento_result_term_sum = 0.0;
    let mut memento_misses = Vec::new();

    let mut simple_hits = 0usize;
    let mut simple_reciprocal_rank_sum = 0.0;
    let mut simple_result_term_sum = 0.0;
    let mut simple_misses = Vec::new();

    let mut memento_only_hits = 0usize;
    let mut simple_only_hits = 0usize;
    let mut memento_latencies = Vec::with_capacity(cases.len());
    let mut simple_latencies = Vec::with_capacity(cases.len());
    let mut per_case = Vec::with_capacity(cases.len());

    for case in &cases {
        let body = serde_json::json!({
            "query": case.query,
            "top_k": top_k,
        });

        let memento_started = Instant::now();
        let response = post("/query", &body.to_string()).await?;
        let memento_latency_ms = memento_started.elapsed().as_secs_f64() * 1_000.0;
        memento_latencies.push(memento_latency_ms);
        let parsed: QueryResponse = serde_json::from_str(&response)?;
        let rank = parsed
            .results
            .iter()
            .position(|result| canonicalize_loose(&result.source_path) == case.expected_path);

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
        let combined_results = parsed
            .results
            .iter()
            .map(|result| result.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        memento_result_term_sum += term_recall(&combined_results, &case.expected_terms);

        let simple_started = Instant::now();
        let simple_results = simple_search(&simple_documents, &case.query, top_k);
        let simple_latency_ms = simple_started.elapsed().as_secs_f64() * 1_000.0;
        simple_latencies.push(simple_latency_ms);
        let simple_rank = simple_results
            .iter()
            .position(|result| result.path == case.expected_path);
        let simple_hit = simple_rank.is_some();
        if let Some(rank) = simple_rank {
            simple_hits += 1;
            simple_reciprocal_rank_sum += 1.0 / (rank as f64 + 1.0);
        } else {
            simple_misses.push(BenchmarkMiss {
                id: case.id.clone(),
                query: case.query.clone(),
                expected_path: case.expected_path.clone(),
                top_result_path: simple_results.first().map(|result| result.path.clone()),
            });
        }
        let simple_combined = simple_results
            .iter()
            .map(|result| result.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        simple_result_term_sum += term_recall(&simple_combined, &case.expected_terms);

        if memento_hit && !simple_hit {
            memento_only_hits += 1;
        } else if simple_hit && !memento_hit {
            simple_only_hits += 1;
        }

        per_case.push(BenchmarkCaseReport {
            id: case.id.clone(),
            memento_rank: rank.map(|value| value + 1),
            simple_rank: simple_rank.map(|value| value + 1),
            memento_top_path: parsed
                .results
                .first()
                .map(|result| result.source_path.clone()),
            simple_top_path: simple_results.first().map(|result| result.path.clone()),
            memento_latency_ms,
            simple_latency_ms,
        });
    }

    let case_count = cases.len().max(1);
    let memento = BenchmarkSummary {
        hits: memento_hits,
        hit_rate: memento_hits as f64 / case_count as f64,
        mrr: memento_reciprocal_rank_sum / case_count as f64,
        avg_answer_term_recall: memento_answer_term_sum / case_count as f64,
        avg_result_term_recall: memento_result_term_sum / case_count as f64,
        latency_ms: summarize_latencies(&memento_latencies),
        misses: memento_misses.into_iter().take(12).collect(),
    };
    let simple_search = BenchmarkSummary {
        hits: simple_hits,
        hit_rate: simple_hits as f64 / case_count as f64,
        mrr: simple_reciprocal_rank_sum / case_count as f64,
        avg_answer_term_recall: 0.0,
        avg_result_term_recall: simple_result_term_sum / case_count as f64,
        latency_ms: summarize_latencies(&simple_latencies),
        misses: simple_misses.into_iter().take(12).collect(),
    };
    let report_data = BenchmarkReport {
        dataset: dataset_path.display().to_string(),
        cases: cases.len(),
        top_k,
        comparison: BenchmarkComparison {
            hit_rate_delta: memento.hit_rate - simple_search.hit_rate,
            mrr_delta: memento.mrr - simple_search.mrr,
            answer_term_recall_delta: memento.avg_answer_term_recall
                - simple_search.avg_answer_term_recall,
            result_term_recall_delta: memento.avg_result_term_recall
                - simple_search.avg_result_term_recall,
            memento_only_hits,
            simple_only_hits,
        },
        memento,
        simple_search,
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
        report_data.memento.avg_answer_term_recall
    );
    println!(
        "memento result term recall: {:.3}",
        report_data.memento.avg_result_term_recall
    );
    println!(
        "simple hit@{}: {:.1}%",
        top_k,
        report_data.simple_search.hit_rate * 100.0
    );
    println!("simple mrr: {:.3}", report_data.simple_search.mrr);
    println!(
        "simple latency: avg {:.1} ms, p50 {:.1} ms, p95 {:.1} ms",
        report_data.simple_search.latency_ms.average,
        report_data.simple_search.latency_ms.p50,
        report_data.simple_search.latency_ms.p95
    );
    println!(
        "simple result term recall: {:.3}",
        report_data.simple_search.avg_result_term_recall
    );
    println!(
        "delta hit@{}: {:+.1}%",
        top_k,
        report_data.comparison.hit_rate_delta * 100.0
    );
    println!("delta mrr: {:+.3}", report_data.comparison.mrr_delta);

    Ok(())
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
    text.split(|c: char| !c.is_alphanumeric())
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

fn common_vault_root(cases: &[BenchmarkCase]) -> Result<PathBuf> {
    let mut iter = cases.iter();
    let first = iter
        .next()
        .context("Benchmark dataset does not contain any cases")?;
    let mut prefix = PathBuf::from(&first.expected_path);
    if prefix.is_file() {
        prefix.pop();
    }

    for case in iter {
        let mut candidate = PathBuf::from(&case.expected_path);
        if candidate.is_file() {
            candidate.pop();
        }
        while !candidate.starts_with(&prefix) {
            if !prefix.pop() {
                return Err(anyhow::anyhow!(
                    "Could not infer common vault root from benchmark dataset"
                ));
            }
        }
    }

    Ok(prefix)
}

fn load_simple_search_documents(cases: &[BenchmarkCase]) -> Result<Vec<SimpleSearchDocument>> {
    let root = common_vault_root(cases)?;
    let mut docs = Vec::new();
    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        let path = entry.into_path();
        if !path.is_file() || !is_markdown(&path) {
            continue;
        }
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let title = extract_title(&path, &contents);
        docs.push(SimpleSearchDocument {
            path: canonicalize_loose(&path.display().to_string()),
            title_tokens: tokenize(&title),
            path_tokens: tokenize(
                &path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .replace(['-', '_'], " "),
            ),
            content_tokens: tokenize(&contents),
            title,
            content: contents,
        });
    }
    Ok(docs)
}

fn simple_search<'a>(
    documents: &'a [SimpleSearchDocument],
    query: &str,
    top_k: usize,
) -> Vec<&'a SimpleSearchDocument> {
    let mut query_terms = tokenize(query);
    query_terms.retain(|term| term.len() >= 2 && !is_generic_term(term));
    query_terms.sort();
    query_terms.dedup();

    let phrase = query_terms.join(" ");
    let mut scored = documents
        .iter()
        .filter_map(|doc| {
            let title_matches = query_terms
                .iter()
                .filter(|term| doc.title_tokens.contains(term))
                .count() as f64;
            let path_matches = query_terms
                .iter()
                .filter(|term| doc.path_tokens.contains(term))
                .count() as f64;
            let content_matches = query_terms
                .iter()
                .filter(|term| doc.content_tokens.contains(term))
                .count() as f64;
            let title_text = doc.title.to_lowercase();
            let phrase_bonus = if !phrase.is_empty() && title_text.contains(&phrase) {
                2.5
            } else {
                0.0
            };
            let score =
                (title_matches * 3.0) + (path_matches * 2.0) + content_matches + phrase_bonus;
            if score > 0.0 {
                Some((score, doc))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(top_k).map(|(_, doc)| doc).collect()
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("md" | "markdown")
    )
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/").or_else(|| path.strip_prefix(r"~\")) {
        return home_dir().join(stripped);
    }
    PathBuf::from(path)
}

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("Could not determine home directory")
}

fn data_dir() -> PathBuf {
    std::env::var_os("MEMENTO_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".memento"))
}

fn pid_path() -> PathBuf {
    data_dir().join("mementod.pid")
}

async fn ensure_daemon() -> Result<()> {
    if connect().await.is_ok() {
        return Ok(());
    }

    fs::create_dir_all(data_dir())?;
    if !daemon_process_is_alive() {
        start_daemon()?;
    }

    for _ in 0..120 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if connect().await.is_ok() {
            return Ok(());
        }
    }

    connect().await.context("mementod did not start")?;
    Ok(())
}

fn daemon_process_is_alive() -> bool {
    let pid_file = pid_path();
    if !pid_file.exists() {
        return false;
    }
    if let Ok(pid_str) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            return process_is_alive(pid);
        }
    }
    false
}

fn process_is_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }

    #[cfg(unix)]
    unsafe {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        kill(pid, 0) == 0
    }

    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32);
        if handle.is_null() {
            return false;
        }
        let mut exit_code = 0_u32;
        let result = GetExitCodeProcess(handle, &mut exit_code);
        let _ = CloseHandle(handle);
        result != 0 && exit_code == STILL_ACTIVE as u32
    }
}

fn start_daemon() -> Result<()> {
    let exe = which_mementod()?;

    #[cfg(unix)]
    Command::new(&exe)
        .arg("--foreground")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to start mementod")?;

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;

        Command::new(&exe)
            .arg("--foreground")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .context("Failed to start mementod")?;
    }
    Ok(())
}

fn which_mementod() -> Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(Path::new("."));
        let candidate = dir.join(format!("mementod{}", std::env::consts::EXE_SUFFIX));
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    let local_target = PathBuf::from("target")
        .join("debug")
        .join(format!("mementod{}", std::env::consts::EXE_SUFFIX));
    if local_target.exists() {
        return Ok(local_target);
    }
    Ok(PathBuf::from(format!(
        "mementod{}",
        std::env::consts::EXE_SUFFIX
    )))
}

#[cfg(unix)]
async fn connect_stream() -> Result<UnixStream> {
    let socket = memento_ipc::unix_socket_path(&data_dir());
    UnixStream::connect(&socket)
        .await
        .with_context(|| format!("Cannot connect to mementod at {}", socket.display()))
}

#[cfg(windows)]
async fn connect_stream() -> Result<NamedPipeClient> {
    let pipe = memento_ipc::windows_pipe_name(&data_dir());
    memento_ipc::connect_windows_pipe(&pipe)
        .await
        .with_context(|| format!("Cannot connect to mementod at {pipe}"))
}

async fn connect() -> Result<hyper::client::conn::http1::SendRequest<Full<Bytes>>> {
    let stream = connect_stream().await?;

    let io = TokioIo::new(stream);
    let (sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .context("HTTP handshake failed")?;

    tokio::spawn(async move {
        let _ = conn.await;
    });

    Ok(sender)
}

async fn post(path: &str, body: &str) -> Result<String> {
    let mut sender = connect().await?;
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("Host", "localhost")
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))?;
    let resp = sender.send_request(req).await?;
    read_body(resp).await
}

async fn read_body(resp: hyper::Response<Incoming>) -> Result<String> {
    let status = resp.status();
    let body = resp.into_body().collect().await?.to_bytes();
    let text = String::from_utf8_lossy(&body).to_string();
    if !status.is_success() {
        return Err(anyhow::anyhow!("HTTP {}: {}", status, text));
    }
    Ok(text)
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
        build_query, common_vault_root, derive_keywords, extract_excerpt, extract_title,
        looks_like_journal_title, process_is_alive, simple_search, summarize_latencies,
        term_recall, BenchmarkCase, SimpleSearchDocument,
    };
    use std::path::{Path, PathBuf};

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

    #[test]
    fn common_root_uses_shared_vault_prefix() {
        let cases = vec![
            BenchmarkCase {
                id: "a".into(),
                query: "q".into(),
                expected_path: "/tmp/vault/memory/a.md".into(),
                expected_title: "A".into(),
                expected_terms: vec![],
                excerpt: String::new(),
            },
            BenchmarkCase {
                id: "b".into(),
                query: "q".into(),
                expected_path: "/tmp/vault/memory/nested/b.md".into(),
                expected_title: "B".into(),
                expected_terms: vec![],
                excerpt: String::new(),
            },
        ];

        assert_eq!(
            common_vault_root(&cases).unwrap(),
            PathBuf::from("/tmp/vault/memory")
        );
    }

    #[test]
    fn simple_search_prefers_title_match() {
        let docs = vec![
            SimpleSearchDocument {
                path: "/tmp/a.md".into(),
                title: "Jose Roberto IT Profile".into(),
                title_tokens: vec!["jose".into(), "roberto".into(), "profile".into()],
                path_tokens: vec!["jose".into(), "roberto".into(), "itprofile".into()],
                content_tokens: vec!["focused".into(), "profile".into()],
                content: "Focused profile".into(),
            },
            SimpleSearchDocument {
                path: "/tmp/b.md".into(),
                title: "Daily Note".into(),
                title_tokens: vec!["daily".into(), "note".into()],
                path_tokens: vec!["daily".into(), "note".into()],
                content_tokens: vec!["jose".into(), "roberto".into(), "profile".into()],
                content: "Jose Roberto profile mention".into(),
            },
        ];

        let results = simple_search(&docs, "what does Jose Roberto IT Profile say?", 5);
        assert_eq!(results.first().unwrap().path, "/tmp/a.md");
    }
}
