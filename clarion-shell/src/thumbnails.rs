// SPDX-License-Identifier: GPL-3.0-or-later
//! Window thumbnails for the Dock.
//!
//! The Shell owns a second Wayland connection for capture, exactly as the
//! clipboard service owns one for data-control. Window discovery, minimized
//! state and the user's actions stay on the normal Clarion IPC path; this
//! module only maps a Clarion window id to its `ext-foreign-toplevel` handle
//! and asks `ext-image-copy-capture` for a frame.
//!
//! Capture is deliberately SHM-only. The compositor renders the window at its
//! native size, the service downscales the frame here, and the Dock receives a
//! small PNG. A menu with a few visible cards does not justify the DMA-BUF
//! import machinery, and a failed capture only affects the preview.

use std::collections::{BTreeSet, HashMap};
use std::io::Cursor;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use async_channel::{Receiver, Sender, TryRecvError};
use image::{
    imageops::{self, FilterType},
    DynamicImage, ImageFormat, RgbaImage,
};
use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    protocol::{
        wl_buffer::WlBuffer, wl_output, wl_registry, wl_shm, wl_shm::WlShm,
        wl_shm_pool::WlShmPool,
    },
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
};
use wayland_protocols::ext::{
    foreign_toplevel_list::v1::client::{
        ext_foreign_toplevel_handle_v1::{self as toplevel_handle, ExtForeignToplevelHandleV1},
        ext_foreign_toplevel_list_v1::{self as toplevel_list, ExtForeignToplevelListV1},
    },
    image_capture_source::v1::client::{
        ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
        ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1::{self as capture_frame, ExtImageCopyCaptureFrameV1},
        ext_image_copy_capture_manager_v1::{
            self as capture_manager, ExtImageCopyCaptureManagerV1,
        },
        ext_image_copy_capture_session_v1::{self as capture_session, ExtImageCopyCaptureSessionV1},
    },
};

/// Longest edge kept for a Dock card. A 250pt-wide card at 2x needs 500px, so
/// one cap serves both the horizontal row and a future larger hover preview.
pub const MAX_DIMENSION: u32 = 512;
/// The menu asks for roughly one frame per second. The compositor and the
/// window's own commit rate decide whether the content is actually new.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(1000);
/// A stopped or failed session is retried after this delay instead of spinning
/// on a window the compositor cannot capture right now.
const RETRY_DELAY: Duration = Duration::from_millis(2000);
/// Images larger than this are rejected before allocating shared memory.
const MAX_FRAME_BYTES: usize = 256 * 1024 * 1024;
const MAX_FRAME_EDGE: u32 = 16384;
/// A menu opening can have several cards ready at once. Cap concurrent
/// compositor captures so the first paint does not queue every native-size
/// window in one frame; the rest start on the next loop iteration.
const MAX_CONCURRENT_CAPTURES: usize = 2;

/// One request from the Shell. The generation makes late frames cheap to drop
/// when the Dock menu closes or switches applications.
#[derive(Clone, Debug)]
pub enum Command {
    SetTargets { generation: u64, ids: Vec<u64> },
    Shutdown,
}

/// One result from the capture thread.
#[derive(Clone, Debug)]
pub enum Event {
    Frame {
        generation: u64,
        id: u64,
        png: Vec<u8>,
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
    pub fn set_targets(&self, generation: u64, ids: Vec<u64>) {
        if self.enabled {
            let _ = self
                .commands
                .try_send(Command::SetTargets { generation, ids });
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

/// Start the capture thread. Preview mode opens no Wayland connection, so
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
    let connection = Connection::connect_to_env().context("无法连接 Wayland 缩略图服务")?;
    let (globals, mut queue) =
        registry_queue_init::<State>(&connection).context("无法初始化 Wayland 缩略图注册表")?;
    let qh = queue.handle();

    let shm = globals.bind::<WlShm, _, _>(&qh, 1..=1, ());
    let list = globals.bind::<ExtForeignToplevelListV1, _, _>(&qh, 1..=1, ());
    let sources =
        globals.bind::<ExtForeignToplevelImageCaptureSourceManagerV1, _, _>(&qh, 1..=1, ());
    let manager = globals.bind::<ExtImageCopyCaptureManagerV1, _, _>(&qh, 1..=1, ());
    let mut missing = Vec::new();
    if shm.is_err() {
        missing.push("wl_shm");
    }
    if list.is_err() {
        missing.push("ext_foreign_toplevel_list_v1");
    }
    if sources.is_err() {
        missing.push("ext_foreign_toplevel_image_capture_source_manager_v1");
    }
    if manager.is_err() {
        missing.push("ext_image_copy_capture_manager_v1");
    }
    if !missing.is_empty() {
        let _ = events.send_blocking(Event::Unavailable {
            reason: format!("missing-protocols:{}", missing.join(",")),
        });
        return Ok(());
    }
    let (shm, list, sources, manager) = (
        shm.expect("checked"),
        list.expect("checked"),
        sources.expect("checked"),
        manager.expect("checked"),
    );

    let mut state = State {
        events,
        shm,
        _list: list,
        sources,
        manager,
        windows: HashMap::new(),
        by_id: HashMap::new(),
        targets: BTreeSet::new(),
        sessions: HashMap::new(),
        retry_at: HashMap::new(),
        generation: 0,
        unavailable: false,
    };
    // The initial roundtrip delivers every existing toplevel before the Dock
    // can ask for one.
    queue.roundtrip(&mut state)?;

    loop {
        loop {
            match commands.try_recv() {
                Ok(Command::SetTargets { generation, ids }) => {
                    state.set_targets(&qh, generation, ids);
                }
                Ok(Command::Shutdown) | Err(TryRecvError::Closed) => return Ok(()),
                Err(TryRecvError::Empty) => break,
            }
        }
        state.reconcile(&qh);
        state.tick(&qh);
        queue.dispatch_pending(&mut state)?;
        connection.flush()?;
        if let Some(guard) = queue.prepare_read() {
            let mut poll = libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // This also bounds how long a command waits behind an idle display.
            if unsafe { libc::poll(&mut poll, 1, 20) } > 0 {
                guard.read()?;
            } else {
                drop(guard);
            }
        }
    }
}

struct State {
    events: Sender<Event>,
    shm: WlShm,
    _list: ExtForeignToplevelListV1,
    sources: ExtForeignToplevelImageCaptureSourceManagerV1,
    manager: ExtImageCopyCaptureManagerV1,
    windows: HashMap<wayland_client::backend::ObjectId, WindowEntry>,
    by_id: HashMap<u64, ExtForeignToplevelHandleV1>,
    targets: BTreeSet<u64>,
    sessions: HashMap<u64, Session>,
    retry_at: HashMap<u64, Instant>,
    generation: u64,
    unavailable: bool,
}

struct WindowEntry {
    _handle: ExtForeignToplevelHandleV1,
    id: Option<u64>,
}

impl State {
    fn set_targets(&mut self, qh: &QueueHandle<Self>, generation: u64, ids: Vec<u64>) {
        self.generation = generation;
        self.targets = ids.into_iter().collect();
        self.retry_at.retain(|id, _| self.targets.contains(id));
        self.reconcile(qh);
    }

    fn reconcile(&mut self, qh: &QueueHandle<Self>) {
        if self.unavailable {
            return;
        }
        let wanted = self.targets.clone();
        let stale: Vec<u64> = self
            .sessions
            .keys()
            .copied()
            .filter(|id| !wanted.contains(id))
            .collect();
        for id in stale {
            if let Some(session) = self.sessions.remove(&id) {
                session.destroy();
            }
        }
        for id in wanted {
            if self.sessions.contains_key(&id) {
                continue;
            }
            if self
                .retry_at
                .get(&id)
                .is_some_and(|until| Instant::now() < *until)
            {
                continue;
            }
            self.retry_at.remove(&id);
            let Some(handle) = self.by_id.get(&id).cloned() else {
                continue;
            };
            let session = self.create_session(qh, id, &handle);
            self.sessions.insert(id, session);
        }
    }

    fn create_session(
        &self,
        qh: &QueueHandle<Self>,
        id: u64,
        handle: &ExtForeignToplevelHandleV1,
    ) -> Session {
        let source = self.sources.create_source(handle, qh, ());
        let session =
            self.manager
                .create_session(&source, capture_manager::Options::empty(), qh, id);
        Session {
            source,
            session,
            buffer: None,
            frame: None,
            pending: Constraints::default(),
            constraints: None,
            next_constraints: None,
            batch: false,
            formats_seen: false,
            in_flight: false,
            ready_at: Instant::now(),
            retries: 0,
            transform: WEnum::Value(wl_output::Transform::Normal),
            failed_until: None,
        }
    }

    fn tick(&mut self, qh: &QueueHandle<Self>) {
        let now = Instant::now();
        let in_flight = self
            .sessions
            .values()
            .filter(|session| session.in_flight)
            .count();
        let budget = MAX_CONCURRENT_CAPTURES.saturating_sub(in_flight);
        if budget == 0 {
            return;
        }
        let ready: Vec<u64> = self
            .sessions
            .iter()
            .filter(|(_, session)| session.wants_capture(now))
            .map(|(id, _)| *id)
            .take(budget)
            .collect();
        for id in ready {
            let Some(session) = self.sessions.get_mut(&id) else {
                continue;
            };
            if session.batch {
                continue;
            }
            let Some(constraints) = session.constraints else {
                continue;
            };
            if session.buffer.is_none() {
                session.buffer = ShmBuffer::new(&self.shm, qh, constraints);
                if session.buffer.is_none() {
                    // Unsupported size/format or a failed memfd. Do not retry
                    // at the loop's poll cadence.
                    session.failed_until = Some(Instant::now() + RETRY_DELAY);
                    continue;
                }
            }
            let Some(buffer) = session.buffer.as_ref() else {
                continue;
            };
            let frame = session.session.create_frame(qh, id);
            frame.attach_buffer(&buffer.buffer);
            frame.damage_buffer(0, 0, constraints.width as i32, constraints.height as i32);
            frame.capture();
            session.frame = Some(frame);
            session.in_flight = true;
        }
    }

    fn fail_session(&mut self, id: u64) {
        if let Some(session) = self.sessions.get_mut(&id) {
            session.in_flight = false;
            session.buffer = None;
            session.failed_until = Some(Instant::now() + RETRY_DELAY);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Constraints {
    width: u32,
    height: u32,
    argb: bool,
    xrgb: bool,
}

impl Constraints {
    fn format(self) -> Option<wl_shm::Format> {
        if self.argb {
            Some(wl_shm::Format::Argb8888)
        } else if self.xrgb {
            Some(wl_shm::Format::Xrgb8888)
        } else {
            None
        }
    }
}

struct Session {
    source: ExtImageCaptureSourceV1,
    session: ExtImageCopyCaptureSessionV1,
    buffer: Option<ShmBuffer>,
    frame: Option<ExtImageCopyCaptureFrameV1>,
    pending: Constraints,
    constraints: Option<Constraints>,
    next_constraints: Option<Constraints>,
    batch: bool,
    formats_seen: bool,
    in_flight: bool,
    ready_at: Instant,
    retries: u8,
    transform: WEnum<wl_output::Transform>,
    failed_until: Option<Instant>,
}

impl Session {
    fn encoded_frame(&mut self) -> Option<Vec<u8>> {
        let buffer = self.buffer.as_ref()?;
        let png = buffer.encoded(self.transform);
        self.ready_at = Instant::now()
            + if png.is_some() {
                FRAME_INTERVAL
            } else {
                RETRY_DELAY
            };
        png
    }

    /// Apply a resize that arrived while the previous frame was still in
    /// flight. The old buffer cannot be released until the compositor has
    /// finished reading it.
    fn apply_pending_constraints(&mut self) {
        if let Some(next) = self.next_constraints.take() {
            self.buffer = None;
            self.constraints = Some(next);
            self.ready_at = Instant::now();
        }
    }

    fn wants_capture(&self, now: Instant) -> bool {
        !self.in_flight
            && self.failed_until.is_none_or(|until| now >= until)
            && now >= self.ready_at
    }

    fn destroy(self) {
        if let Some(frame) = self.frame {
            frame.destroy();
        }
        drop(self.buffer);
        self.session.destroy();
        self.source.destroy();
    }
}

/// One shared-memory buffer reused for every frame of a session. The protocol
/// only requires a new frame after the previous one reaches `ready`/`failed`,
/// so a single buffer is enough.
struct ShmBuffer {
    buffer: WlBuffer,
    pixels: *mut u8,
    len: usize,
    width: u32,
    height: u32,
    format: wl_shm::Format,
}

impl ShmBuffer {
    fn new(shm: &WlShm, qh: &QueueHandle<State>, constraints: Constraints) -> Option<Self> {
        let format = constraints.format()?;
        let width = constraints.width;
        let height = constraints.height;
        if width == 0 || height == 0 || width > MAX_FRAME_EDGE || height > MAX_FRAME_EDGE {
            return None;
        }
        let stride = width.checked_mul(4)?;
        let len = (stride as usize).checked_mul(height as usize)?;
        if len == 0 || len > MAX_FRAME_BYTES {
            return None;
        }
        let fd = unsafe { libc::memfd_create(c"clarion-thumbnail".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return None;
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        if unsafe { libc::ftruncate(fd.as_raw_fd(), len as libc::off_t) } != 0 {
            return None;
        }
        let pixels = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if pixels == libc::MAP_FAILED {
            return None;
        }
        let pool = shm.create_pool(fd.as_fd(), len as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            format,
            qh,
            (),
        );
        pool.destroy();
        Some(Self {
            buffer,
            pixels: pixels.cast(),
            len,
            width,
            height,
            format,
        })
    }

    fn encoded(&self, transform: WEnum<wl_output::Transform>) -> Option<Vec<u8>> {
        if transform != WEnum::Value(wl_output::Transform::Normal) {
            return None;
        }
        let raw = unsafe { std::slice::from_raw_parts(self.pixels, self.len) };
        encode_thumbnail(
            raw,
            self.width,
            self.height,
            MAX_DIMENSION,
            self.format == wl_shm::Format::Argb8888,
        )
        .map(|(png, _, _)| png)
    }
}

impl Drop for ShmBuffer {
    fn drop(&mut self) {
        self.buffer.destroy();
        unsafe {
            libc::munmap(self.pixels.cast(), self.len);
        }
    }
}

/// Convert one captured SHM frame to a small PNG. `ARGB8888` in Wayland is
/// little-endian BGRA in memory, so the red and blue channels swap once.
fn encode_thumbnail(
    raw: &[u8],
    width: u32,
    height: u32,
    max: u32,
    alpha: bool,
) -> Option<(Vec<u8>, u32, u32)> {
    if width == 0 || height == 0 || raw.len() < (width as usize) * (height as usize) * 4 {
        return None;
    }
    let (source_width, source_height) = (width, height);
    let (target_width, target_height) = fit(width, height, max);
    // Wayland's ARGB8888 is little-endian BGRA. Swap the two colour channels
    // in place before handing the buffer to `image`; the full frame is copied
    // once, then only the small result is kept.
    let mut rgba = raw[..(source_width as usize) * (source_height as usize) * 4].to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
        if !alpha {
            pixel[3] = 255;
        }
    }
    let source = RgbaImage::from_raw(source_width, source_height, rgba)?;
    let scaled = imageops::resize(&source, target_width, target_height, FilterType::Triangle);
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(scaled)
        .write_to(&mut output, ImageFormat::Png)
        .ok()?;
    Some((output.into_inner(), target_width, target_height))
}

/// Preserve the source aspect ratio and never enlarge.
fn fit(width: u32, height: u32, max: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= max || longest == 0 {
        (width, height)
    } else {
        let width = ((width as u64 * max as u64) / longest as u64).max(1) as u32;
        let height = ((height as u64 * max as u64) / longest as u64).max(1) as u32;
        (width, height)
    }
}

// ---------------------------------------------------------------- dispatch --

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // `GlobalList` owns registry bookkeeping; the capture service reacts
        // when a bound global is removed through its own proxies below.
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for State {
    fn event(
        state: &mut Self,
        _list: &ExtForeignToplevelListV1,
        event: toplevel_list::Event,
        _data: &(),
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            toplevel_list::Event::Toplevel { toplevel } => {
                state.windows.insert(
                    toplevel.id(),
                    WindowEntry {
                        _handle: toplevel,
                        id: None,
                    },
                );
            }
            toplevel_list::Event::Finished => {
                state.unavailable = true;
                let _ = state.events.try_send(Event::Unavailable {
                    reason: "toplevel-list-finished".into(),
                });
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(State, ExtForeignToplevelListV1, [
        toplevel_list::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        handle: &ExtForeignToplevelHandleV1,
        event: toplevel_handle::Event,
        _data: &(),
        _connection: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            toplevel_handle::Event::Identifier { identifier } => {
                if let Some(entry) = state.windows.get_mut(&handle.id()) {
                    entry.id = identifier.parse().ok();
                }
            }
            toplevel_handle::Event::Done => {
                let Some(entry) = state.windows.get_mut(&handle.id()) else {
                    return;
                };
                if let Some(id) = entry.id {
                    state.by_id.insert(id, handle.clone());
                }
                state.reconcile(qh);
            }
            toplevel_handle::Event::Closed => {
                let id = state
                    .windows
                    .remove(&handle.id())
                    .and_then(|entry| entry.id);
                if let Some(id) = id {
                    state.by_id.remove(&id);
                    if let Some(session) = state.sessions.remove(&id) {
                        session.destroy();
                    }
                    let _ = state.events.try_send(Event::Closed { id });
                }
                handle.destroy();
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, u64> for State {
    fn event(
        state: &mut Self,
        _session: &ExtImageCopyCaptureSessionV1,
        event: capture_session::Event,
        id: &u64,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(session) = state.sessions.get_mut(id) else {
            return;
        };
        match event {
            capture_session::Event::BufferSize { width, height } => {
                if !session.batch {
                    // A resize batch may only resend the size; keep the
                    // formats the session negotiated earlier.
                    session.pending = session.constraints.unwrap_or_default();
                    session.batch = true;
                    session.formats_seen = false;
                }
                session.pending.width = width;
                session.pending.height = height;
            }
            capture_session::Event::ShmFormat { format } => {
                if !session.batch {
                    session.pending = session.constraints.unwrap_or_default();
                    session.batch = true;
                    session.formats_seen = false;
                }
                if !session.formats_seen {
                    // A batch that resends formats replaces the negotiated
                    // set instead of OR-ing a removed format back in.
                    session.pending.argb = false;
                    session.pending.xrgb = false;
                    session.formats_seen = true;
                }
                session.pending.argb |= matches!(format, WEnum::Value(wl_shm::Format::Argb8888));
                session.pending.xrgb |= matches!(format, WEnum::Value(wl_shm::Format::Xrgb8888));
            }
            capture_session::Event::Done => {
                let next = session.pending;
                session.batch = false;
                session.formats_seen = false;
                if session.constraints != Some(next) {
                    if session.in_flight {
                        session.next_constraints = Some(next);
                    } else {
                        session.buffer = None;
                        session.constraints = Some(next);
                        session.ready_at = Instant::now();
                    }
                }
            }
            capture_session::Event::Stopped => {
                if let Some(session) = state.sessions.remove(id) {
                    session.destroy();
                }
                state.retry_at.insert(*id, Instant::now() + RETRY_DELAY);
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, u64> for State {
    fn event(
        state: &mut Self,
        frame: &ExtImageCopyCaptureFrameV1,
        event: capture_frame::Event,
        id: &u64,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            capture_frame::Event::Transform { transform } => {
                if let Some(session) = state.sessions.get_mut(id) {
                    session.transform = transform;
                }
            }
            capture_frame::Event::Ready => {
                frame.destroy();
                let Some(session) = state.sessions.get_mut(id) else {
                    return;
                };
                session.frame = None;
                session.in_flight = false;
                if session.transform != WEnum::Value(wl_output::Transform::Normal) {
                    state.fail_session(*id);
                    return;
                }
                if let Some(png) = session.encoded_frame() {
                    let _ = state.events.try_send(Event::Frame {
                        generation: state.generation,
                        id: *id,
                        png,
                    });
                }
                session.apply_pending_constraints();
            }
            capture_frame::Event::Failed { reason } => {
                frame.destroy();
                if let Some(session) = state.sessions.get_mut(id) {
                    session.frame = None;
                }
                let retry = matches!(
                    reason,
                    WEnum::Value(capture_frame::FailureReason::BufferConstraints)
                );
                if let Some(session) = state.sessions.get_mut(id) {
                    session.in_flight = false;
                    if retry {
                        session.retries = session.retries.saturating_add(1);
                        session.buffer = None;
                        session.failed_until = if session.retries < 3 {
                            Some(Instant::now())
                        } else {
                            Some(Instant::now() + RETRY_DELAY)
                        };
                    } else {
                        session.failed_until = Some(Instant::now() + RETRY_DELAY);
                    }
                    session.apply_pending_constraints();
                }
            }
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(State:ignore WlShm);
wayland_client::delegate_noop!(State:ignore WlShmPool);
wayland_client::delegate_noop!(State:ignore WlBuffer);
wayland_client::delegate_noop!(State:ignore ExtImageCaptureSourceV1);
wayland_client::delegate_noop!(State:ignore ExtForeignToplevelImageCaptureSourceManagerV1);
wayland_client::delegate_noop!(State:ignore ExtImageCopyCaptureManagerV1);

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;

    #[test]
    fn fit_preserves_aspect_and_never_enlarges() {
        assert_eq!(fit(1920, 1080, 512), (512, 288));
        assert_eq!(fit(800, 1200, 512), (341, 512));
        assert_eq!(fit(320, 200, 512), (320, 200));
        assert_eq!(fit(0, 0, 512), (0, 0));
    }

    #[test]
    fn bgra_frame_becomes_rgba_png() {
        // Two pixels: opaque red and half-transparent blue, stored BGRA.
        let raw = [
            0, 0, 255, 255, // red
            255, 0, 0, 128, // blue
        ];
        let (png, width, height) = encode_thumbnail(&raw, 2, 1, 512, true).expect("encoded");
        assert_eq!((width, height), (2, 1));
        let decoded = image::load_from_memory(&png).expect("png").to_rgba8();
        assert_eq!(decoded.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
        assert_eq!(decoded.get_pixel(1, 0), &Rgba([0, 0, 255, 128]));
    }

    #[test]
    fn xrgb_frames_are_opaque() {
        let raw = [0, 0, 255, 0, 0, 0, 0, 0];
        let (png, _, _) = encode_thumbnail(&raw, 2, 1, 512, false).expect("encoded");
        let decoded = image::load_from_memory(&png).expect("png").to_rgba8();
        assert_eq!(decoded.get_pixel(0, 0).0[3], 255);
        assert_eq!(decoded.get_pixel(1, 0).0[3], 255);
    }

    #[test]
    fn a_short_buffer_is_rejected() {
        assert!(encode_thumbnail(&[0; 3], 2, 1, 512, true).is_none());
    }

    /// Live protocol smoke test. Run against a real Clarion/niri session with:
    /// `CLARION_THUMBNAIL_TEST_ID=<window-id> cargo test -p clarion-shell \
    ///  live_window_capture -- --ignored --nocapture`
    #[test]
    #[ignore = "requires a live Clarion Wayland session and a window id"]
    fn live_window_capture() {
        let id: u64 = std::env::var("CLARION_THUMBNAIL_TEST_ID")
            .expect("set CLARION_THUMBNAIL_TEST_ID")
            .parse()
            .expect("numeric window id");
        let (events, receiver) = async_channel::bounded(16);
        let service = start(events, false);
        service.set_targets(1, vec![id]);
        let (std_tx, std_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(event) = receiver.recv_blocking() {
                if std_tx.send(event).is_err() {
                    break;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let event = std_rx
                .recv_timeout(remaining)
                .expect("no thumbnail frame before the deadline");
            match event {
                Event::Frame {
                    generation,
                    id: got,
                    png,
                } => {
                    assert_eq!(generation, 1);
                    assert_eq!(got, id);
                    let image = image::load_from_memory(&png).expect("valid png");
                    assert!(image.width() > 0 && image.height() > 0);
                    assert!(
                        image.width().max(image.height()) <= MAX_DIMENSION,
                        "thumbnail was not downscaled: {}x{}",
                        image.width(),
                        image.height()
                    );
                    println!(
                        "captured window {}: {}x{} png ({} bytes)",
                        id,
                        image.width(),
                        image.height(),
                        png.len()
                    );
                    return;
                }
                Event::Unavailable { reason } => panic!("capture unavailable: {reason}"),
                Event::Closed { id: closed } => panic!("window {closed} closed during capture"),
            }
        }
    }
}
