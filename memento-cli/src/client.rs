//! Client — connects to mementod over the platform-local IPC transport.

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::Request;
use std::path::PathBuf;

#[cfg(windows)]
use tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
use tokio::net::UnixStream;

use crate::config;

pub fn data_dir() -> PathBuf {
    config::data_dir()
}

pub fn endpoint_description() -> String {
    memento_ipc::endpoint_description(&data_dir())
}

pub fn endpoint_exists() -> bool {
    #[cfg(unix)]
    {
        return memento_ipc::unix_socket_path(&data_dir()).exists();
    }

    #[cfg(windows)]
    {
        return daemon_process_is_alive();
    }

    #[allow(unreachable_code)]
    false
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

    #[cfg(windows)]
    {
        use std::fs::OpenOptions;
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;

        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("Failed to open daemon log {}", log_path.display()))?;
        let stderr = stdout
            .try_clone()
            .context("Failed to clone daemon log handle")?;
        let mut command = std::process::Command::new(exe);
        command
            .arg("--foreground")
            .stdin(std::process::Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
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
    if pid <= 0 {
        return false;
    }

    #[cfg(unix)]
    unsafe {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        // Signal 0 checks process existence without sending a signal.
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
        let candidate = dir.join(format!("mementod{}", std::env::consts::EXE_SUFFIX));
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
