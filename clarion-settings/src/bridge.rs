//! Client for the Shell's private command socket. The Shell remains the only
//! writer of `preferences.json`; this app only sends partial updates.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;

const TIMEOUT: Duration = Duration::from_millis(700);

#[derive(Clone, Debug)]
pub struct ShellBridge {
    path: PathBuf,
}

impl ShellBridge {
    pub fn from_environment() -> Self {
        Self::at(socket_path(&std::env::vars().collect()))
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn connected(&self) -> bool {
        self.path.exists()
    }

    pub fn patch(&self, patch: &Value) -> Result<()> {
        if patch.as_object().is_none_or(|object| object.is_empty()) {
            return Ok(());
        }
        self.request(&format!("preferences-patch {}", serde_json::to_string(patch)?))
    }

    /// Ask the Shell to apply one wallpaper from the settings history.
    pub fn command(&self, command: &str) -> Result<()> {
        self.request(command)
    }

    pub fn set_wallpaper(&self, path: &Path) -> Result<()> {
        let encoded = serde_json::to_string(&path.to_string_lossy())?;
        self.request(&format!("set-wallpaper {encoded}"))
    }

    fn request(&self, command: &str) -> Result<()> {
        let mut stream = UnixStream::connect(&self.path)
            .with_context(|| format!("connect to {}", self.path.display()))?;
        stream.set_write_timeout(Some(TIMEOUT))?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        stream.write_all(command.as_bytes())?;
        stream.write_all(b"\n")?;
        let mut reader = BufReader::new(stream);
        let mut reply = String::new();
        reader.read_line(&mut reply)?;
        anyhow::ensure!(
            reply.trim() == "ok",
            "Shell rejected {command}: {}",
            reply.trim()
        );
        Ok(())
    }
}

fn socket_path(environment: &HashMap<String, String>) -> PathBuf {
    for key in ["CLARION_SHELL_SOCKET", "REALAND_SHELL_SOCKET"] {
        if let Some(path) = environment.get(key).filter(|path| !path.is_empty()) {
            return PathBuf::from(path);
        }
    }
    environment
        .get("XDG_RUNTIME_DIR")
        .filter(|runtime| runtime.starts_with('/'))
        .map(PathBuf::from)
        .map(|runtime| runtime.join("clarion-shell.sock"))
        .unwrap_or_else(|| {
            PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() }))
                .join("clarion-shell.sock")
        })
}
