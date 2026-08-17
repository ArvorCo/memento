//! Client — connects to mementod via Unix socket

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::Request;
use std::path::PathBuf;
use tokio::net::UnixStream;

use crate::config;

pub fn data_dir() -> PathBuf {
    config::data_dir()
}

pub fn socket_path() -> PathBuf {
    data_dir().join("memento.sock")
}

pub fn start_daemon() -> Result<()> {
    let exe = which_mementod()?;
    let log_path = data_dir().join("mementod.log");

    #[cfg(unix)]
    {
        let command = format!(
            "nohup {} --foreground >> {} 2>&1 &",
            shell_quote(exe.to_string_lossy().as_ref()),
            shell_quote(log_path.to_string_lossy().as_ref())
        );
        std::process::Command::new("sh")
            .arg("-lc")
            .arg(command)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("Failed to start mementod via nohup shell launcher")?;
    }

    #[cfg(not(unix))]
    {
        std::process::Command::new(exe)
            .arg("--foreground")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("Failed to start mementod")?;
    }

    Ok(())
}

pub async fn ensure_daemon_running(timeout: std::time::Duration) -> Result<()> {
    if wait_for_daemon_ready(std::time::Duration::from_millis(800))
        .await
        .is_ok()
    {
        return Ok(());
    }

    if daemon_process_is_alive() {
        return wait_for_daemon_ready(timeout).await;
    }

    eprintln!("Starting mementod...");
    start_daemon()?;
    wait_for_daemon_ready(timeout).await
}

fn daemon_process_is_alive() -> bool {
    let pid_path = data_dir().join("mementod.pid");
    let Ok(raw) = std::fs::read_to_string(pid_path) else {
        return false;
    };
    let Ok(pid) = raw.trim().parse::<i32>() else {
        return false;
    };

    process_is_alive(pid)
}

fn process_is_alive(pid: i32) -> bool {
    #[cfg(unix)]
    unsafe {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        // Signal 0 checks process existence without sending a signal.
        kill(pid, 0) == 0
    }

    #[cfg(not(unix))]
    return false;
}

pub async fn wait_for_daemon_ready(timeout: std::time::Duration) -> Result<()> {
    let started = std::time::Instant::now();
    let mut last_error = None;

    while started.elapsed() < timeout {
        match get("/health").await {
            Ok(_) => return Ok(()),
            Err(err) => {
                last_error = Some(err);
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
        }
    }

    match last_error {
        Some(err) => Err(err).context("mementod did not become ready in time"),
        None => Err(anyhow::anyhow!("mementod did not become ready in time")),
    }
}

fn which_mementod() -> Result<PathBuf> {
    // Try same directory as memento binary
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(std::path::Path::new("."));
        let candidate = dir.join("mementod");
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    // Try PATH
    Ok(PathBuf::from("mementod"))
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

async fn connect() -> Result<hyper::client::conn::http1::SendRequest<Full<Bytes>>> {
    let sock = socket_path();
    let stream = UnixStream::connect(&sock)
        .await
        .with_context(|| format!("Cannot connect to mementod at {}", sock.display()))?;

    let io = hyper_util::rt::TokioIo::new(stream);
    let (sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .context("HTTP handshake failed")?;

    tokio::spawn(async move {
        if let Err(e) = conn.await {
            eprintln!("Connection error: {e}");
        }
    });

    Ok(sender)
}

pub async fn get(path: &str) -> Result<String> {
    let mut sender = connect().await?;
    let req = Request::builder()
        .uri(path)
        .header("Host", "localhost")
        .body(Full::new(Bytes::new()))?;
    let resp = sender.send_request(req).await?;
    read_body(resp).await
}

pub async fn post(path: &str, body: &str) -> Result<String> {
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

#[cfg(test)]
mod tests {
    use super::process_is_alive;

    #[test]
    fn current_process_is_reported_alive() {
        assert!(process_is_alive(std::process::id() as i32));
    }
}
