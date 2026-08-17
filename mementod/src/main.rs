//! mementod — Memento daemon
//!
//! Background service managing .memento files.
//! Serves API on Unix socket (~/.memento/memento.sock) + optional HTTP.

use anyhow::Result;
use clap::Parser;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::signal;
use tracing::{info, warn};
use uuid::Uuid;

mod api;
mod ignore_rules;
mod manager;
mod memory_classification;
mod operation_checkpoint;
mod query_scoring;
mod recovery_snapshot;
mod scheduler;
mod server;
mod text_utils;

#[derive(Parser)]
#[command(
    name = "mementod",
    about = "Memento daemon — background service managing .memento files",
    version
)]
struct Cli {
    /// Optional HTTP port (in addition to Unix socket)
    #[arg(long)]
    http_port: Option<u16>,

    /// HTTP bind host (default: 127.0.0.1). Non-loopback hosts require --allow-remote-http.
    #[arg(long, default_value = "127.0.0.1")]
    http_host: String,

    /// Explicitly allow exposing HTTP beyond loopback.
    #[arg(long, default_value_t = false)]
    allow_remote_http: bool,

    /// Data directory (default: ~/.memento/)
    #[arg(long)]
    data_dir: Option<PathBuf>,

    /// Run in foreground (don't daemonize)
    #[arg(long, short)]
    foreground: bool,
}

fn data_dir(cli_override: Option<&PathBuf>) -> PathBuf {
    cli_override.cloned().unwrap_or_else(|| {
        std::env::var_os("MEMENTO_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .expect("Could not determine home directory")
                    .join(".memento")
            })
    })
}

fn socket_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("memento.sock")
}

fn pid_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("mementod.pid")
}

fn http_token_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("config").join("http_auth_token")
}

struct RuntimeFilesGuard {
    socket_path: PathBuf,
    pid_path: PathBuf,
}

impl RuntimeFilesGuard {
    fn new(socket_path: PathBuf, pid_path: PathBuf) -> Self {
        Self {
            socket_path,
            pid_path,
        }
    }
}

impl Drop for RuntimeFilesGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_file(&self.pid_path);
    }
}

fn is_loopback_http_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

fn write_pid_file(path: &Path) -> Result<()> {
    fs::write(path, std::process::id().to_string())?;
    Ok(())
}

fn ensure_http_auth_token(data_dir: &std::path::Path) -> Result<String> {
    if let Some(value) = std::env::var_os("MEMENTO_HTTP_TOKEN") {
        let token = value.to_string_lossy().trim().to_string();
        if token.is_empty() {
            anyhow::bail!("MEMENTO_HTTP_TOKEN is set but empty");
        }
        return Ok(token);
    }

    let path = http_token_path(data_dir);
    if path.exists() {
        let token = fs::read_to_string(&path)?.trim().to_string();
        if token.is_empty() {
            anyhow::bail!("HTTP auth token file is empty: {}", path.display());
        }
        return Ok(token);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let token = format!("memento_{}", Uuid::new_v4().simple());
    let mut file = fs::File::create(&path)?;
    file.write_all(token.as_bytes())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(token)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let dir = data_dir(cli.data_dir.as_ref());

    if cli.http_port.is_some() && !cli.allow_remote_http && !is_loopback_http_host(&cli.http_host) {
        anyhow::bail!(
            "Refusing to bind HTTP on non-loopback host {} without --allow-remote-http",
            cli.http_host
        );
    }

    // Ensure data directory exists
    fs::create_dir_all(&dir)?;

    let pid_file = pid_path(&dir);
    let sock = socket_path(&dir);
    let _runtime_files = RuntimeFilesGuard::new(sock.clone(), pid_file.clone());

    // Clean up stale socket
    if sock.exists() {
        fs::remove_file(&sock)?;
    }

    // Publish the live PID before loading a potentially large memory store so a
    // second CLI invocation waits for this process instead of racing another
    // daemon through startup.
    write_pid_file(&pid_file)?;

    // Initialize manager
    let mgr = std::sync::Arc::new(manager::MementoManager::new(&dir)?);

    info!(
        "mementod starting — socket: {}, data: {}",
        sock.display(),
        dir.display()
    );

    // Build router
    scheduler::start_scheduler(std::sync::Arc::clone(&mgr), dir.clone()).await?;
    let app = server::build_router(std::sync::Arc::clone(&mgr));
    let unix_listener = server::bind_unix_socket(&sock)?;

    let http_listener = if let Some(port) = cli.http_port {
        let http_token = ensure_http_auth_token(&dir)?;
        let http_app = server::with_http_auth(app.clone(), http_token.clone().into());
        let http_addr = format!("{}:{port}", cli.http_host);
        let listener = server::bind_http(&cli.http_host, port).await?;

        info!("HTTP server on {http_addr}");
        info!(
            "HTTP auth enabled — bearer token path: {}",
            http_token_path(&dir).display()
        );

        Some((http_app, listener, http_addr))
    } else {
        None
    };

    // Start servers
    let socket_handle = server::serve_unix_socket(app, unix_listener, &sock);

    let http_handle = if let Some((http_app, listener, addr)) = http_listener {
        Some(server::serve_http(http_app, listener, addr))
    } else {
        None
    };

    // Wait for shutdown signal
    tokio::select! {
        result = socket_handle => {
            if let Err(e) = result {
                warn!("Socket server error: {e}");
            }
        }
        result = async { match http_handle { Some(h) => h.await, None => std::future::pending().await } } => {
            if let Err(e) = result {
                warn!("HTTP server error: {e}");
            }
        }
        _ = shutdown_signal() => {
            info!("Shutdown signal received");
        }
    }

    info!("mementod shut down cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = signal::ctrl_c();
    let mut term = signal::unix::signal(signal::unix::SignalKind::terminate())
        .expect("Failed to install SIGTERM handler");

    tokio::select! {
        _ = ctrl_c => {},
        _ = term.recv() => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_guard_accepts_only_local_hosts_by_default() {
        assert!(is_loopback_http_host("127.0.0.1"));
        assert!(is_loopback_http_host("::1"));
        assert!(is_loopback_http_host("localhost"));
        assert!(!is_loopback_http_host("0.0.0.0"));
        assert!(!is_loopback_http_host("192.168.1.10"));
    }

    #[test]
    fn runtime_files_guard_removes_runtime_artifacts_on_drop() {
        let temp = tempfile::tempdir().unwrap();
        let socket_path = temp.path().join("memento.sock");
        let pid_path = temp.path().join("mementod.pid");

        fs::write(&socket_path, "").unwrap();
        fs::write(&pid_path, "1234").unwrap();

        {
            let _guard = RuntimeFilesGuard::new(socket_path.clone(), pid_path.clone());
        }

        assert!(!socket_path.exists());
        assert!(!pid_path.exists());
    }
}
