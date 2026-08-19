mod benchmark;

use crate::benchmark::{build_benchmark, run_benchmark};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(
    name = "memento-research",
    about = "Research and backend diagnostics for Memento",
    version = env!("CARGO_PKG_VERSION"),
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect the current machine and recommend the best research backend
    Doctor {
        /// Backend preference override
        #[arg(long, value_enum, default_value = "auto")]
        backend: BackendPreference,
        /// Output JSON instead of plain text
        #[arg(long)]
        json: bool,
    },
    /// Print the architecture contract for the research service
    Plan {
        /// Backend preference override
        #[arg(long, value_enum, default_value = "auto")]
        backend: BackendPreference,
        /// Vault path the experiments should target
        #[arg(long)]
        vault: Option<String>,
    },
    /// Probe a backend runner directly
    Probe {
        /// Backend preference override
        #[arg(long, value_enum, default_value = "auto")]
        backend: BackendPreference,
    },
    /// Build or run retrieval benchmarks
    Benchmark {
        #[command(subcommand)]
        command: BenchmarkCommands,
    },
}

#[derive(Subcommand)]
enum BenchmarkCommands {
    /// Generate benchmark cases from a vault
    Build {
        /// Vault path used as source material
        #[arg(long)]
        vault: String,
        /// Output dataset path in JSONL format
        #[arg(long, default_value = "research/benchmarks/dataset.jsonl")]
        output: String,
        /// Maximum markdown files to sample
        #[arg(long, default_value_t = 250)]
        limit: usize,
    },
    /// Run a dataset against the live daemon
    Run {
        /// Benchmark dataset path in JSONL format
        #[arg(long, default_value = "research/benchmarks/dataset.jsonl")]
        dataset: String,
        /// Explicit corpus root shared by Memento and the lexical baseline
        #[arg(long)]
        corpus: String,
        /// Max query results to ask from the daemon
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        /// Optional limit on benchmark cases
        #[arg(long)]
        limit: Option<usize>,
        /// Complete unmeasured suite passes before timing
        #[arg(long, default_value_t = 1)]
        warmup: usize,
        /// Measured observations per case
        #[arg(long, default_value_t = 3)]
        repetitions: usize,
        /// Report file written as JSON
        #[arg(long, default_value = "research/reports/latest.json")]
        report: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum BackendPreference {
    Auto,
    Mlx,
    Nvidia,
    Cpu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum BackendKind {
    Mlx,
    Nvidia,
    Cpu,
}

#[derive(Clone, Debug, Serialize)]
struct CapabilityReport {
    os: String,
    arch: String,
    is_apple_silicon: bool,
    python_executable: Option<String>,
    python_available: bool,
    uv: bool,
    mlx_module: bool,
    torch_module: bool,
    torch_mps_available: bool,
    nvidia_smi: bool,
    cuda_visible: bool,
    metal_gpu_reported: bool,
    recommended_backend: BackendKind,
    notes: Vec<String>,
    suggested_bootstrap: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProbeResult {
    backend: BackendKind,
    ok: bool,
    detail: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Doctor { backend, json } => {
            let report = detect_capabilities(backend);
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_report(&report);
            }
        }
        Commands::Plan { backend, vault } => {
            let report = detect_capabilities(backend);
            print_plan(&report, vault.as_deref());
        }
        Commands::Probe { backend } => {
            let result = probe_backend(backend)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::Benchmark { command } => match command {
            BenchmarkCommands::Build {
                vault,
                output,
                limit,
            } => build_benchmark(&vault, &output, limit)?,
            BenchmarkCommands::Run {
                dataset,
                corpus,
                top_k,
                limit,
                warmup,
                repetitions,
                report,
            } => {
                run_benchmark(
                    &dataset,
                    &corpus,
                    top_k,
                    limit,
                    warmup,
                    repetitions,
                    &report,
                )
                .await?
            }
        },
    }

    Ok(())
}

fn detect_capabilities(preference: BackendPreference) -> CapabilityReport {
    let os = run_capture("uname", ["-s"]).unwrap_or_else(|| std::env::consts::OS.to_string());
    let arch = run_capture("uname", ["-m"]).unwrap_or_else(|| std::env::consts::ARCH.to_string());
    let python_executable = preferred_python();
    let python_available = python_executable.is_some();
    let uv = command_exists("uv");
    let nvidia_smi = command_exists("nvidia-smi");
    let cuda_visible = if nvidia_smi {
        run_status("nvidia-smi", ["--query-gpu=name", "--format=csv,noheader"])
    } else {
        false
    };

    let is_apple_silicon = os == "Darwin" && arch == "arm64";
    let metal_gpu_reported = if os == "Darwin" {
        run_capture("system_profiler", ["SPDisplaysDataType"])
            .map(|output| output.contains("Metal"))
            .unwrap_or(false)
    } else {
        false
    };

    let mlx_module = python_available
        && python_eval(
            python_executable.as_deref(),
            "import importlib.util; raise SystemExit(0 if importlib.util.find_spec('mlx') else 1)",
        );
    let torch_module = python_available
        && python_eval(
            python_executable.as_deref(),
            "import importlib.util; raise SystemExit(0 if importlib.util.find_spec('torch') else 1)",
        );
    let torch_mps_available = python_available
        && torch_module
        && python_eval(
            python_executable.as_deref(),
            "import torch; raise SystemExit(0 if torch.backends.mps.is_available() else 1)",
        );

    let mut notes = Vec::new();
    if is_apple_silicon {
        notes.push("Apple Silicon detectado; o caminho preferencial para GPU local e treinamento leve e MLX.".to_string());
    }
    if metal_gpu_reported {
        notes.push(
            "O macOS reportou suporte Metal; isso e pre-requisito para MLX e para MPS.".to_string(),
        );
    }
    if mlx_module {
        notes.push("O modulo Python `mlx` ja esta disponivel neste host.".to_string());
    } else if is_apple_silicon {
        notes.push(
            "O modulo Python `mlx` ainda nao esta instalado; sem isso o backend MLX nao sobe."
                .to_string(),
        );
    }
    if torch_mps_available {
        notes.push("PyTorch com backend MPS esta disponivel; isso serve como fallback no Mac, mas MLX tende a encaixar melhor no Apple Silicon.".to_string());
    }
    if cuda_visible {
        notes.push("GPU NVIDIA detectada via `nvidia-smi`; backend CUDA pode ser suportado em hosts Linux apropriados.".to_string());
    }
    if !python_available {
        notes.push(
            "Nenhum runtime Python compativel foi encontrado; os runners Python de pesquisa nao poderao iniciar."
                .to_string(),
        );
    }
    if !uv {
        notes.push(
            "`uv` nao foi encontrado; o bootstrap de ambientes reproduziveis fica mais fraco."
                .to_string(),
        );
    }

    let recommended_backend = choose_backend(preference, is_apple_silicon, cuda_visible);

    let suggested_bootstrap = bootstrap_commands(recommended_backend);

    CapabilityReport {
        os,
        arch,
        is_apple_silicon,
        python_executable,
        python_available,
        uv,
        mlx_module,
        torch_module,
        torch_mps_available,
        nvidia_smi,
        cuda_visible,
        metal_gpu_reported,
        recommended_backend,
        notes,
        suggested_bootstrap,
    }
}

fn probe_backend(preference: BackendPreference) -> Result<ProbeResult> {
    let report = detect_capabilities(preference);
    let backend = report.recommended_backend;

    let result = match backend {
        BackendKind::Mlx => {
            let python = report
                .python_executable
                .as_deref()
                .context("No Python executable available for MLX probe")?;
            let detail = run_capture(
                python,
                ["-c", "import mlx.core as mx; print(mx.default_device())"],
            )
            .context("MLX probe failed")?;
            ProbeResult {
                backend,
                ok: detail.contains("gpu"),
                detail,
            }
        }
        BackendKind::Nvidia => ProbeResult {
            backend,
            ok: run_status("nvidia-smi", ["--query-gpu=name", "--format=csv,noheader"]),
            detail: run_capture("nvidia-smi", ["--query-gpu=name", "--format=csv,noheader"])
                .unwrap_or_else(|| "nvidia-smi unavailable".to_string()),
        },
        BackendKind::Cpu => ProbeResult {
            backend,
            ok: true,
            detail: "cpu fallback".to_string(),
        },
    };

    Ok(result)
}

fn choose_backend(
    preference: BackendPreference,
    is_apple_silicon: bool,
    cuda_visible: bool,
) -> BackendKind {
    match preference {
        BackendPreference::Mlx => BackendKind::Mlx,
        BackendPreference::Nvidia => BackendKind::Nvidia,
        BackendPreference::Cpu => BackendKind::Cpu,
        BackendPreference::Auto => {
            if is_apple_silicon {
                BackendKind::Mlx
            } else if cuda_visible {
                BackendKind::Nvidia
            } else {
                BackendKind::Cpu
            }
        }
    }
}

fn bootstrap_commands(backend: BackendKind) -> Vec<String> {
    match backend {
        BackendKind::Mlx => vec![
            "cd research".to_string(),
            "uv venv .venv".to_string(),
            "uv pip install --python .venv/bin/python mlx mlx-lm".to_string(),
            "cargo run -p memento-research -- doctor --backend mlx".to_string(),
            "cargo run -p memento-research -- probe --backend mlx".to_string(),
        ],
        BackendKind::Nvidia => vec![
            "cd research".to_string(),
            "uv venv .venv".to_string(),
            "uv pip install --python .venv/bin/python -U torch --index-url https://download.pytorch.org/whl/cu128".to_string(),
            "cargo run -p memento-research -- doctor --backend nvidia".to_string(),
        ],
        BackendKind::Cpu => vec![
            "cd research".to_string(),
            "uv venv .venv".to_string(),
            "cargo run -p memento-research -- doctor --backend cpu".to_string(),
        ],
    }
}

fn print_report(report: &CapabilityReport) {
    println!("Memento Research Doctor");
    println!("-----------------------");
    println!("host: {} {}", report.os, report.arch);
    println!(
        "recommended backend: {}",
        backend_label(report.recommended_backend)
    );
    println!(
        "python executable: {}",
        report.python_executable.as_deref().unwrap_or("not found")
    );
    println!("python runtime: {}", yes_no(report.python_available));
    println!("uv: {}", yes_no(report.uv));
    println!("mlx module: {}", yes_no(report.mlx_module));
    println!("torch module: {}", yes_no(report.torch_module));
    println!("torch mps: {}", yes_no(report.torch_mps_available));
    println!("nvidia-smi: {}", yes_no(report.nvidia_smi));
    println!("cuda visible: {}", yes_no(report.cuda_visible));
    println!("metal reported: {}", yes_no(report.metal_gpu_reported));

    if !report.notes.is_empty() {
        println!();
        println!("notes:");
        for note in &report.notes {
            println!("- {}", note);
        }
    }

    if !report.suggested_bootstrap.is_empty() {
        println!();
        println!("suggested bootstrap:");
        for step in &report.suggested_bootstrap {
            println!("  {}", step);
        }
    }
}

fn print_plan(report: &CapabilityReport, vault: Option<&str>) {
    let vault = vault.unwrap_or("<set with --vault /path/to/your/vault>");

    println!("Memento Research Plan");
    println!("---------------------");
    println!("service: memento-research");
    println!("folder: research/");
    println!("backend: {}", backend_label(report.recommended_backend));
    println!("vault target: {}", vault);
    println!();
    println!("1. Sync the vault into mementod with incremental manifests.");
    println!("2. Build evaluation sets from real queries and accepted chunks.");
    println!("3. Run backend-specific experiments that tune chunking, retrieval, and eigenvector hyperparameters.");
    println!("4. Keep only changes that improve grounded retrieval quality.");
    println!();
    println!("backend contract:");
    println!("- mlx: Apple Silicon runner using Metal via MLX for local experiments.");
    println!("- nvidia: CUDA runner for Linux hosts with NVIDIA GPUs.");
    println!("- cpu: deterministic fallback for CI, smoke tests, and unsupported machines.");
    println!();
    println!("service boundary:");
    println!("- mementod remains the source of truth for ingest, sync, learn, and query.");
    println!(
        "- memento-research orchestrates experiments, benchmarks, and backend-specific runners."
    );
    println!("- research/ stores protocols, notes, and backend bootstrapping material.");
}

fn backend_label(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::Mlx => "mlx",
        BackendKind::Nvidia => "nvidia",
        BackendKind::Cpu => "cpu",
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn command_exists(command: &str) -> bool {
    run_status(
        "sh",
        ["-lc", &format!("command -v {} >/dev/null 2>&1", command)],
    )
}

fn preferred_python() -> Option<String> {
    if let Some(path) = std::env::var_os("MEMENTO_RESEARCH_PYTHON") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path.display().to_string());
        }
    }

    let repo_venv = PathBuf::from(".venv").join("bin").join("python");
    if repo_venv.exists() {
        return Some(repo_venv.display().to_string());
    }

    let research_venv = PathBuf::from("research")
        .join(".venv")
        .join("bin")
        .join("python");
    if research_venv.exists() {
        return Some(research_venv.display().to_string());
    }

    run_capture("sh", ["-lc", "command -v python"])
}

fn python_eval(python: Option<&str>, code: &str) -> bool {
    let Some(python) = python else {
        return false;
    };
    run_status(python, ["-c", code])
}

fn run_capture<const N: usize>(program: &str, args: [&str; N]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(stdout.trim().to_string())
}

fn run_status<const N: usize>(program: &str, args: [&str; N]) -> bool {
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{choose_backend, BackendKind, BackendPreference};

    #[test]
    fn auto_prefers_mlx_on_apple_silicon() {
        let backend = choose_backend(BackendPreference::Auto, true, false);
        assert_eq!(backend, BackendKind::Mlx);
    }

    #[test]
    fn auto_prefers_nvidia_when_cuda_is_visible() {
        let backend = choose_backend(BackendPreference::Auto, false, true);
        assert_eq!(backend, BackendKind::Nvidia);
    }

    #[test]
    fn explicit_cpu_override_wins() {
        let backend = choose_backend(BackendPreference::Cpu, true, true);
        assert_eq!(backend, BackendKind::Cpu);
    }
}
