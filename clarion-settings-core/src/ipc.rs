//! Minimal synchronous IPC plus the trait the configuration manager uses.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs};

use anyhow::{Context, Result};
use clarion_ipc::{Action, Output, Reply, Request, Response, TilingMode, Window, Workspace};


// IPC paths are interpreted in the compositor's working directory.
pub fn config_reload_action(path: &std::path::Path) -> Result<Action> {
    let path = std::path::absolute(path).context("Cannot resolve config path")?;
    let path = path
        .to_str()
        .context("Config path must be valid UTF-8 for IPC")?;
    Ok(Action::LoadConfigFile {
        path: Some(path.to_owned()),
    })
}

/// The synchronous operations configuration writes need from the compositor.
pub trait ConfigIpc {
    fn perform_action(&self, action: Action) -> Result<()>;
    #[allow(clippy::type_complexity)]
    fn request(
        &self,
        request: Request,
    ) -> Result<std::sync::mpsc::Receiver<std::result::Result<Response, String>>>;
}

pub struct Connection {
    socket_path: Option<PathBuf>,
}

impl Connection {
    pub fn new() -> Self {
        let socket_path = Self::discover_socket();
        Self { socket_path }
    }

    pub fn with_socket_path(path: Option<PathBuf>) -> Self {
        Self { socket_path: path }
    }

    pub fn is_connected(&self) -> bool {
        self.socket_path.is_some()
    }

    pub fn socket_path(&self) -> Option<&PathBuf> {
        self.socket_path.as_ref()
    }

    fn discover_socket() -> Option<PathBuf> {
        // An explicit session socket is authoritative. Do not accidentally send actions to a
        // different session when its socket is stale or the selected compositor is unavailable.
        if let Some(path) = clarion_ipc::socket::socket_path_from_environment() {
            return crate::socket_io::connect(&path).ok().map(|_| path);
        }

        // 2. Scan XDG_RUNTIME_DIR for clarion.*.sock
        if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR") {
            if let Ok(entries) = fs::read_dir(&runtime_dir) {
                let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
                paths.sort();
                for prefix in ["clarion.", "realand."] {
                    for path in &paths {
                        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                            continue;
                        };
                        if name.starts_with(prefix)
                            && name.ends_with(".sock")
                            && crate::socket_io::connect(path).is_ok()
                        {
                            return Some(path.clone());
                        }
                    }
                }
            }
        }

        None
    }

    pub fn connect(&self) -> Result<UnixStream> {
        let path = self
            .socket_path
            .as_ref()
            .context("Clarion IPC socket not found")?;
        crate::socket_io::connect(path).context("Failed to connect to Clarion IPC socket")
    }

    pub fn send(&self, request: Request) -> Result<Response> {
        // Modesets may take longer than ordinary IPC. This runs on the worker,
        // while the compositor owns the independent 15-second recovery timer.
        let timeout = if matches!(
            request,
            Request::PreviewOutputConfig { .. }
                | Request::ConfirmOutputPreview { .. }
                | Request::CancelOutputPreview { .. }
        ) {
            Duration::from_secs(16)
        } else {
            crate::socket_io::TIMEOUT
        };
        let mut socket = self.connect()?;
        socket.set_read_timeout(Some(timeout))?;
        let mut request = serde_json::to_vec(&request)?;
        request.push(b'\n');
        socket.write_all(&request)?;
        let response = crate::socket_io::read_line(&mut socket, 16 * 1024 * 1024, timeout)?;
        let reply: Reply = serde_json::from_slice(&response)?;
        match reply {
            Ok(resp) => Ok(resp),
            Err(e) => anyhow::bail!("Clarion IPC error: {e}"),
        }
    }

    pub fn reload_config_path(&self, path: &std::path::Path) -> Result<()> {
        self.perform_action(config_reload_action(path)?)
    }
    pub fn reload_config(&self) -> Result<()> {
        self.perform_action(Action::LoadConfigFile { path: None })
    }

    pub fn switch_tiling_mode(&self, mode: Option<TilingMode>) -> Result<()> {
        self.perform_action(Action::SwitchTilingMode { mode })
    }

    pub fn toggle_overview(&self) -> Result<()> {
        self.perform_action(Action::ToggleOverview {})
    }

    pub fn toggle_expo(&self) -> Result<()> {
        self.perform_action(Action::ToggleExpo {})
    }

    pub fn perform_action(&self, action: Action) -> Result<()> {
        match self.send(Request::Action(action))? {
            Response::Handled => Ok(()),
            _ => anyhow::bail!("Unexpected action response"),
        }
    }

    pub fn get_windows(&self) -> Result<Vec<Window>> {
        match self.send(Request::Windows)? {
            Response::Windows(windows) => Ok(windows),
            _ => anyhow::bail!("Unexpected windows response"),
        }
    }

    pub fn get_outputs(&self) -> Result<Vec<Output>> {
        match self.send(Request::Outputs)? {
            Response::Outputs(outputs) => Ok(outputs.into_values().collect()),
            _ => anyhow::bail!("Unexpected outputs response"),
        }
    }

    pub fn get_workspaces(&self) -> Result<Vec<Workspace>> {
        match self.send(Request::Workspaces)? {
            Response::Workspaces(workspaces) => Ok(workspaces),
            _ => anyhow::bail!("Unexpected workspaces response"),
        }
    }

    pub fn get_version(&self) -> Result<String> {
        match self.send(Request::Version)? {
            Response::Version(version) => Ok(version),
            _ => anyhow::bail!("Unexpected version response"),
        }
    }

    pub fn management_state(&self) -> Result<clarion_ipc::management::ManagementState> {
        match self.send(Request::ManagementState)? {
            Response::ManagementState(json) => Ok(serde_json::from_str(&json)?),
            _ => anyhow::bail!("运行中的 Clarion 尚不支持布局管理，请更新后重新登录"),
        }
    }
}


impl Default for Connection {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigIpc for Connection {
    fn perform_action(&self, action: Action) -> Result<()> {
        Connection::perform_action(self, action)
    }

    fn request(
        &self,
        request: Request,
    ) -> Result<std::sync::mpsc::Receiver<std::result::Result<Response, String>>> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let _ = sender.send(self.send(request).map_err(|error| error.to_string()));
        Ok(receiver)
    }
}

/// Scripted IPC for the configuration tests: replies are delivered
/// synchronously through the same channel the manager polls.
#[cfg(test)]
pub struct TestIpc {
    handler: Box<dyn Fn(Request) -> std::result::Result<Response, String> + Send + Sync>,
    connected: bool,
}

#[cfg(test)]
impl TestIpc {
    pub fn scripted(
        handler: impl Fn(Request) -> std::result::Result<Response, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            handler: Box::new(handler),
            connected: true,
        }
    }

    pub fn disconnected() -> Self {
        Self {
            handler: Box::new(|_| Err("not connected".into())),
            connected: false,
        }
    }
}

#[cfg(test)]
impl ConfigIpc for TestIpc {
    fn perform_action(&self, action: Action) -> Result<()> {
        if !self.connected {
            anyhow::bail!("not connected");
        }
        let response = (self.handler)(Request::Action(action)).map_err(|error| anyhow::anyhow!(error))?;
        match response {
            Response::Handled => Ok(()),
            _ => anyhow::bail!("unexpected action response"),
        }
    }

    fn request(
        &self,
        request: Request,
    ) -> Result<std::sync::mpsc::Receiver<std::result::Result<Response, String>>> {
        if !self.connected {
            anyhow::bail!("not connected");
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let _ = sender.send((self.handler)(request));
        Ok(receiver)
    }
}
