//! Cross-platform daemon lifecycle and local IPC for benchmarks.

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::Request;
use hyper_util::rt::TokioIo;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(windows)]
use tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
use tokio::net::UnixStream;

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

pub(super) async fn ensure_daemon() -> Result<()> {
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

pub(super) fn process_is_alive(pid: i32) -> bool {
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

pub(super) async fn post(path: &str, body: &str) -> Result<String> {
    let mut sender = connect().await?;
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header("Host", "localhost")
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))?;
    let response = sender.send_request(request).await?;
    read_body(response).await
}

async fn read_body(response: hyper::Response<Incoming>) -> Result<String> {
    let status = response.status();
    let body = response.into_body().collect().await?.to_bytes();
    let text = String::from_utf8_lossy(&body).to_string();
    if !status.is_success() {
        return Err(anyhow::anyhow!("HTTP {}: {}", status, text));
    }
    Ok(text)
}
