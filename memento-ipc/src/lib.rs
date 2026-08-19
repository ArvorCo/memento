//! Cross-platform naming for Memento's local IPC endpoint.

use std::path::{Path, PathBuf};

/// Return the Unix-domain socket used by a data directory.
pub fn unix_socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join("memento.sock")
}

/// Return a deterministic Windows named-pipe name for a data directory.
///
/// Windows pipe names cannot contain a filesystem path. Hashing the normalized
/// absolute data directory keeps independent stores isolated while avoiding
/// usernames or other personal path components in the global pipe namespace.
pub fn windows_pipe_name_for_path(data_dir: &Path) -> String {
    let absolute = std::path::absolute(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    let normalized = absolute.to_string_lossy().replace('/', "\\").to_lowercase();
    let hash = fnv1a64(normalized.as_bytes());
    format!(r"\\.\pipe\memento-{hash:016x}")
}

#[cfg(windows)]
pub fn windows_pipe_name(data_dir: &Path) -> String {
    std::env::var("MEMENTO_PIPE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| windows_pipe_name_for_path(data_dir))
}

/// Connect to a Windows named pipe, tolerating the short interval where every
/// existing server instance is busy and the daemon is creating the next one.
#[cfg(windows)]
pub async fn connect_windows_pipe(
    endpoint: &str,
) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeClient> {
    use std::time::Duration;
    use tokio::net::windows::named_pipe::ClientOptions;

    const ERROR_PIPE_BUSY: i32 = 231;
    // A burst of local clients may briefly occupy every server instance while
    // the daemon accepts and replaces them. Keep retrying long enough for a
    // busy CPU-only machine without adding latency to the normal path.
    const ATTEMPTS: usize = 200;

    for attempt in 0..ATTEMPTS {
        match ClientOptions::new().open(endpoint) {
            Ok(client) => return Ok(client),
            Err(error)
                if error.raw_os_error() == Some(ERROR_PIPE_BUSY) && attempt + 1 < ATTEMPTS =>
            {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("the named-pipe retry loop always returns on its final attempt")
}

pub fn endpoint_description(data_dir: &Path) -> String {
    #[cfg(unix)]
    {
        return unix_socket_path(data_dir).display().to_string();
    }

    #[cfg(windows)]
    {
        return windows_pipe_name(data_dir);
    }

    #[allow(unreachable_code)]
    "unsupported local transport".to_string()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_socket_is_scoped_to_data_dir() {
        assert_eq!(
            unix_socket_path(Path::new("/tmp/memento-test")),
            PathBuf::from("/tmp/memento-test/memento.sock")
        );
    }

    #[test]
    fn windows_pipe_name_is_stable_and_path_safe() {
        let first = windows_pipe_name_for_path(Path::new(r"C:\Users\Example\.memento"));
        let second = windows_pipe_name_for_path(Path::new(r"c:\users\example\.memento"));
        assert_eq!(first, second);
        assert!(first.starts_with(r"\\.\pipe\memento-"));
        assert_eq!(first.len(), r"\\.\pipe\memento-".len() + 16);
        assert!(!first.contains("Example"));
    }

    #[test]
    fn windows_pipe_name_separates_data_directories() {
        assert_ne!(
            windows_pipe_name_for_path(Path::new(r"C:\Users\A\.memento")),
            windows_pipe_name_for_path(Path::new(r"C:\Users\B\.memento"))
        );
    }
}
