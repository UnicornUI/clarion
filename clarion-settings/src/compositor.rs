//! Minimal compositor IPC used to raise the settings window after the Shell
//! spawns it. Wayland's activation token is not always applied by winit, so a
//! FocusWindow request by PID makes the launch deterministic.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clarion_ipc::{Action, LogicalOutput, Reply, Request, Response};

const TIMEOUT: Duration = Duration::from_millis(700);

pub fn focus_own_window() -> Result<bool> {
    let Some(path) = socket_path() else {
        return Ok(false);
    };
    let windows = match send(&path, Request::Windows)? {
        Response::Windows(windows) => windows,
        _ => return Ok(false),
    };
    let pid = std::process::id() as i32;
    let Some(window) = windows.iter().find(|window| window.pid == Some(pid)) else {
        return Ok(false);
    };
    let id = window.id;
    match send(&path, Request::Action(Action::FocusWindow { id }))? {
        Response::Handled => Ok(true),
        _ => Ok(false),
    }
}

/// Where the settings window actually sits on its output, as reported by the
/// compositor. A Wayland client cannot know where its toplevel was placed, and
/// GPUI keeps the requested bounds, so popup surfaces must ask the compositor.
pub struct OwnPlacement {
    /// Mutable output geometry, including the origin of its logical view.
    pub output: LogicalOutput,
    /// Window origin in output-local logical pixels.
    pub position: (f64, f64),
}

pub fn own_window_placement() -> Result<Option<OwnPlacement>> {
    let Some(path) = socket_path() else {
        return Ok(None);
    };
    let windows = match send(&path, Request::Windows)? {
        Response::Windows(windows) => windows,
        _ => return Ok(None),
    };
    let pid = std::process::id() as i32;
    let Some(window) = windows
        .iter()
        .find(|window| window.pid == Some(pid) && window.app_id.as_deref() == Some(APP_ID))
    else {
        return Ok(None);
    };
    let Some(workspace_id) = window.workspace_id else {
        return Ok(None);
    };
    let workspaces = match send(&path, Request::Workspaces)? {
        Response::Workspaces(workspaces) => workspaces,
        _ => return Ok(None),
    };
    let Some(output_name) = workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
        .and_then(|workspace| workspace.output.clone())
    else {
        return Ok(None);
    };
    let outputs = match send(&path, Request::Outputs)? {
        Response::Outputs(outputs) => outputs,
        _ => return Ok(None),
    };
    let Some(output) = outputs.get(&output_name).and_then(|output| output.logical) else {
        return Ok(None);
    };
    let Some((tile_x, tile_y)) = window.layout.tile_pos_in_workspace_view else {
        return Ok(None);
    };
    let (offset_x, offset_y) = window.layout.window_offset_in_tile;
    // Workspace view coordinates are local to the output's view, which starts
    // at the output's logical origin.
    Ok(Some(OwnPlacement {
        output,
        position: (tile_x + offset_x, tile_y + offset_y),
    }))
}

pub const APP_ID: &str = "clarion-settings";

fn socket_path() -> Option<PathBuf> {
    if let Some(path) = clarion_ipc::socket::socket_path_from_environment() {
        if UnixStream::connect(&path).is_ok() {
            return Some(path);
        }
    }
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let entries = std::fs::read_dir(runtime).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_str()?;
        if (name.starts_with("clarion.") || name.starts_with("realand."))
            && name.ends_with(".sock")
            && UnixStream::connect(&path).is_ok()
        {
            return Some(path);
        }
    }
    None
}

fn send(path: &PathBuf, request: Request) -> Result<Response> {
    let mut stream = UnixStream::connect(path)
        .with_context(|| format!("connect to {}", path.display()))?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let mut bytes = serde_json::to_vec(&request)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    let mut response = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let length = stream.read(&mut chunk)?;
        if length == 0 {
            break;
        }
        response.extend_from_slice(&chunk[..length]);
        if response.contains(&b'\n') {
            break;
        }
        anyhow::ensure!(response.len() <= 16 * 1024 * 1024, "IPC reply is too large");
    }
    let reply: Reply = serde_json::from_slice(&response)?;
    reply.map_err(|error| anyhow::anyhow!("Clarion IPC error: {error}"))
}
