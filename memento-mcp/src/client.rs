use std::path::PathBuf;

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[cfg(windows)]
use tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
use tokio::net::UnixStream;

#[derive(Debug, Clone)]
pub struct DaemonClient {
    #[cfg(unix)]
    socket_path: PathBuf,
    #[cfg(windows)]
    pipe_name: String,
}

impl DaemonClient {
    pub fn from_environment() -> Self {
        let data_dir = std::env::var_os("MEMENTO_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".memento")))
            .unwrap_or_else(|| PathBuf::from(".memento"));
        #[cfg(unix)]
        {
            let socket_path = std::env::var_os("MEMENTO_SOCKET")
                .map(PathBuf::from)
                .unwrap_or_else(|| memento_ipc::unix_socket_path(&data_dir));
            Self { socket_path }
        }

        #[cfg(windows)]
        {
            let pipe_name = memento_ipc::windows_pipe_name(&data_dir);
            Self { pipe_name }
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request::<(), T>("GET", path, None).await
    }

    pub async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        self.request("POST", path, Some(body)).await
    }

    async fn request<B: Serialize, T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<&B>,
    ) -> Result<T> {
        let stream = self.connect_stream().await?;
        let io = TokioIo::new(stream);
        let (mut sender, connection) = hyper::client::conn::http1::handshake(io)
            .await
            .context("mementod HTTP handshake failed")?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let encoded = body
            .map(serde_json::to_vec)
            .transpose()
            .context("failed to encode request")?
            .unwrap_or_default();
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("Host", "localhost")
            .header("Content-Type", "application/json")
            .body(Full::new(Bytes::from(encoded)))?;
        let response = sender
            .send_request(request)
            .await
            .context("mementod request failed")?;
        let status = response.status();
        let payload = response.into_body().collect().await?.to_bytes();
        if !status.is_success() {
            let detail = String::from_utf8_lossy(&payload);
            anyhow::bail!("mementod returned HTTP {status}: {detail}");
        }
        serde_json::from_slice(&payload).context("mementod returned invalid JSON")
    }

    #[cfg(unix)]
    async fn connect_stream(&self) -> Result<UnixStream> {
        UnixStream::connect(&self.socket_path)
            .await
            .with_context(|| {
                format!(
                    "cannot reach mementod at {}; start the daemon with `mementod --foreground`",
                    self.socket_path.display()
                )
            })
    }

    #[cfg(windows)]
    async fn connect_stream(&self) -> Result<NamedPipeClient> {
        memento_ipc::connect_windows_pipe(&self.pipe_name)
            .await
            .with_context(|| {
                format!(
                    "cannot reach mementod at {}; start the daemon with `mementod --foreground`",
                    self.pipe_name
                )
            })
    }
}
