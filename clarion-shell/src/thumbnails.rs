//! Window thumbnails for the Dock.
//!
//! The Shell asks the compositor over a dedicated IPC request. The compositor
//! renders the window into a small offscreen target on the GPU and returns
//! only the final BGRA pixels, so the native window buffer never crosses the
//! socket and the standard window-capture protocol keeps its full-resolution
//! semantics for recording clients.

use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use async_channel::{Receiver, Sender, TryRecvError};
use base64::Engine as _;
use clarion_ipc::socket::Socket;
use clarion_ipc::{Request, Response};

/// Longest edge kept for a Dock card. The card's thumbnail area is at most
/// ~254pt wide, so 256px covers a 1x card and still survives the smaller row
/// sizes when several windows share the menu.
pub const MAX_DIMENSION: u32 = 256;
/// How often a visible preview refreshes. The capture itself is GPU-scaled,
/// so this is a content-refresh cadence, not a capture cost.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(1000);
/// Commands are cheap; polling only bounds how quickly a target change is
/// noticed while waiting for the next frame.
const IDLE_POLL: Duration = Duration::from_millis(10);

/// One request from the Shell. The generation makes late frames cheap to drop
/// when the Dock menu closes or switches applications.
#[derive(Clone, Debug)]
pub enum Command {
    SetTargets {
        generation: u64,
        ids: Vec<u64>,
        /// Keep refreshing while the preview is visible. A one-shot request
        /// warms the cache for windows the user has not opened yet.
        live: bool,
    },
    Shutdown,
}

/// A downscaled capture in the byte order GPUI's `RenderImage` expects.
#[derive(Clone, Debug)]
pub struct ThumbnailFrame {
    pub width: u32,
    pub height: u32,
    /// Row-major BGRA, straight alpha.
    pub bgra: Vec<u8>,
}

impl ThumbnailFrame {
    /// A solid test frame from an RGBA colour.
    #[cfg(test)]
    pub(crate) fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let bgra = [rgba[2], rgba[1], rgba[0], rgba[3]];
        let mut bytes = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for _ in 0..width * height {
            bytes.extend_from_slice(&bgra);
        }
        Self {
            width,
            height,
            bgra: bytes,
        }
    }
}

/// One result from the capture thread.
#[derive(Clone, Debug)]
pub enum Event {
    Frame {
        generation: u64,
        id: u64,
        frame: ThumbnailFrame,
    },
    Closed {
        id: u64,
    },
    Unavailable {
        reason: String,
    },
}

/// The Shell's handle to the capture thread. Dropping it stops the thread.
pub struct ThumbnailService {
    commands: Sender<Command>,
    enabled: bool,
}

impl ThumbnailService {
    pub fn set_targets(&self, generation: u64, ids: Vec<u64>, live: bool) {
        if self.enabled {
            let _ = self.commands.try_send(Command::SetTargets {
                generation,
                ids,
                live,
            });
        }
    }

    pub fn shutdown(&self) {
        if self.enabled {
            let _ = self.commands.try_send(Command::Shutdown);
        }
    }
}

impl Drop for ThumbnailService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Start the capture thread. Preview mode opens no IPC connection, so
/// `--preview` and unit tests never touch the daily session.
pub fn start(events: Sender<Event>, preview: bool) -> ThumbnailService {
    let (commands, receiver) = async_channel::unbounded();
    if preview {
        return ThumbnailService {
            commands,
            enabled: false,
        };
    }
    let handle = ThumbnailService {
        commands,
        enabled: true,
    };
    let _ = std::thread::Builder::new()
        .name("clarion-thumbnails".into())
        .spawn(move || {
            if let Err(error) = run(events.clone(), receiver) {
                let _ = events.send_blocking(Event::Unavailable {
                    reason: format!("{error:#}"),
                });
            }
        });
    handle
}

fn run(events: Sender<Event>, commands: Receiver<Command>) -> Result<()> {
    let mut socket = Socket::connect().context("无法连接 Clarion 缩略图 IPC")?;
    let mut generation = 0;
    let mut targets: Vec<u64> = Vec::new();
    let mut live = false;
    let mut next: Option<Instant> = None;

    loop {
        loop {
            match commands.try_recv() {
                Ok(Command::SetTargets {
                    generation: next_generation,
                    ids,
                    live: next_live,
                }) => {
                    generation = next_generation;
                    targets = ids;
                    live = next_live;
                    // The first frame should not wait a refresh interval.
                    next = (!targets.is_empty()).then(Instant::now);
                }
                Ok(Command::Shutdown) | Err(TryRecvError::Closed) => return Ok(()),
                Err(TryRecvError::Empty) => break,
            }
        }

        let now = Instant::now();
        let Some(due) = next else {
            std::thread::sleep(IDLE_POLL);
            continue;
        };
        if now < due {
            std::thread::sleep(IDLE_POLL.min(due - now));
            continue;
        }

        let reply = socket.send(Request::WindowThumbnails {
            ids: targets.clone(),
            max: MAX_DIMENSION,
        });
        next = live.then(|| Instant::now() + FRAME_INTERVAL);
        match reply {
            Ok(Ok(Response::WindowThumbnails(thumbnails))) => {
                for (id, thumbnail) in targets.iter().zip(thumbnails) {
                    match thumbnail {
                        Some(thumbnail) => {
                            if thumbnail.id != *id {
                                continue;
                            }
                            let expected =
                                (thumbnail.width as usize) * (thumbnail.height as usize) * 4;
                            let Ok(bgra) = base64::engine::general_purpose::STANDARD
                                .decode(thumbnail.bgra)
                            else {
                                continue;
                            };
                            if bgra.len() != expected {
                                continue;
                            }
                            let _ = events.try_send(Event::Frame {
                                generation,
                                id: *id,
                                frame: ThumbnailFrame {
                                    width: thumbnail.width,
                                    height: thumbnail.height,
                                    bgra,
                                },
                            });
                        }
                        None => {
                            // The window is gone or its output is temporarily
                            // unavailable. Dropping the image lets the card
                            // fall back to the app icon; a later frame restores
                            // it if the window is still there.
                            let _ = events.try_send(Event::Closed { id: *id });
                        }
                    }
                }
            }
            Ok(Err(reason)) => {
                let _ = events.try_send(Event::Unavailable { reason });
                return Ok(());
            }
            Ok(Ok(_)) => {
                let _ = events.try_send(Event::Unavailable {
                    reason: "unexpected thumbnail reply".into(),
                });
                return Ok(());
            }
            Err(error) => {
                let _ = events.try_send(Event::Unavailable {
                    reason: error.to_string(),
                });
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frames_keep_bgra_order() {
        let frame = ThumbnailFrame::solid(2, 1, [255, 0, 0, 255]);
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 1);
        assert_eq!(&frame.bgra[..4], &[0, 0, 255, 255]);
        assert_eq!(&frame.bgra[4..], &[0, 0, 255, 255]);
    }
}
