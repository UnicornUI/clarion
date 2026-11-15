//! End-to-end ext-image-copy-capture output tests using a real Wayland client.

use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clarion_config::{BlockOutFrom, Color, Config, WindowRule};
use clarion_ipc::{Cast, CastKind, CastTarget};
use smithay::reexports::wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};
use smithay::reexports::wayland_protocols::ext::image_capture_source::v1::client::ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1;
use smithay::reexports::wayland_protocols::ext::image_capture_source::v1::client::ext_image_capture_source_v1::ExtImageCaptureSourceV1;
use smithay::reexports::wayland_protocols::ext::image_capture_source::v1::client::ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1;
use smithay::reexports::wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_cursor_session_v1::{
    self, ExtImageCopyCaptureCursorSessionV1,
};
use smithay::reexports::wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::{
    self, ExtImageCopyCaptureFrameV1,
};
use smithay::reexports::wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::{
    ExtImageCopyCaptureManagerV1, Options,
};
use smithay::reexports::wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_session_v1::{
    self, ExtImageCopyCaptureSessionV1,
};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_pointer::WlPointer;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};

use super::client::{ClientId, State as ClientState};
use super::Fixture;

const RGBA: [u8; 4] = [17, 73, 157, 255];

type SessionEvents = Arc<Mutex<SessionState>>;

#[derive(Default)]
struct SessionState {
    size: Option<(u32, u32)>,
    formats: Vec<wl_shm::Format>,
    done: bool,
    done_events: usize,
    stopped: bool,
}

impl Dispatch<ExtImageCopyCaptureSessionV1, SessionEvents> for ClientState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        data: &SessionEvents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let mut events = data.lock().unwrap();
        match event {
            ext_image_copy_capture_session_v1::Event::BufferSize { width, height } => {
                events.size = Some((width, height));
            }
            ext_image_copy_capture_session_v1::Event::ShmFormat { format } => {
                let WEnum::Value(format) = format else {
                    panic!("unknown SHM capture format")
                };
                events.formats.push(format);
            }
            ext_image_copy_capture_session_v1::Event::DmabufDevice { .. } => (),
            ext_image_copy_capture_session_v1::Event::DmabufFormat { .. } => (),
            ext_image_copy_capture_session_v1::Event::Done => {
                events.done = true;
                events.done_events += 1;
            }
            ext_image_copy_capture_session_v1::Event::Stopped => events.stopped = true,
            _ => unreachable!(),
        }
    }
}

type FrameEvents = Arc<Mutex<FrameState>>;

#[derive(Default)]
struct FrameState {
    ready: usize,
    failed: usize,
    failure_reason: Option<WEnum<ext_image_copy_capture_frame_v1::FailureReason>>,
    transform: Option<wayland_client::protocol::wl_output::Transform>,
    damage: Vec<(i32, i32, i32, i32)>,
    presentation_time: Option<(u32, u32, u32)>,
}

impl Dispatch<ExtImageCopyCaptureFrameV1, FrameEvents> for ClientState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        data: &FrameEvents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let mut events = data.lock().unwrap();
        match event {
            ext_image_copy_capture_frame_v1::Event::Transform { transform } => {
                let WEnum::Value(transform) = transform else {
                    panic!("unknown capture transform")
                };
                events.transform = Some(transform);
            }
            ext_image_copy_capture_frame_v1::Event::Damage {
                x,
                y,
                width,
                height,
            } => events.damage.push((x, y, width, height)),
            ext_image_copy_capture_frame_v1::Event::PresentationTime {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
            } => events.presentation_time = Some((tv_sec_hi, tv_sec_lo, tv_nsec)),
            ext_image_copy_capture_frame_v1::Event::Ready => events.ready += 1,
            ext_image_copy_capture_frame_v1::Event::Failed { reason } => {
                events.failed += 1;
                events.failure_reason = Some(reason);
            }
            _ => unreachable!(),
        }
    }
}

type Toplevels = Arc<Mutex<Vec<ExtForeignToplevelHandleV1>>>;

impl Dispatch<ExtForeignToplevelListV1, Toplevels> for ClientState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        data: &Toplevels,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            data.lock().unwrap().push(toplevel);
        }
    }

    wayland_client::event_created_child!(ClientState, ExtForeignToplevelListV1, [
        0 => (ExtForeignToplevelHandleV1, ())
    ]);
}

type CursorEvents = Arc<Mutex<CursorState>>;

#[derive(Default)]
struct CursorState {
    enter: usize,
    leave: usize,
    position: Option<(i32, i32)>,
    hotspot: Option<(i32, i32)>,
}

impl Dispatch<ExtImageCopyCaptureCursorSessionV1, CursorEvents> for ClientState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtImageCopyCaptureCursorSessionV1,
        event: ext_image_copy_capture_cursor_session_v1::Event,
        data: &CursorEvents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let mut events = data.lock().unwrap();
        match event {
            ext_image_copy_capture_cursor_session_v1::Event::Enter => events.enter += 1,
            ext_image_copy_capture_cursor_session_v1::Event::Leave => events.leave += 1,
            ext_image_copy_capture_cursor_session_v1::Event::Position { x, y } => {
                events.position = Some((x, y));
            }
            ext_image_copy_capture_cursor_session_v1::Event::Hotspot { x, y } => {
                events.hotspot = Some((x, y));
            }
            _ => unreachable!(),
        }
    }
}

wayland_client::delegate_noop!(ClientState: ignore ExtOutputImageCaptureSourceManagerV1);
wayland_client::delegate_noop!(ClientState: ignore ExtForeignToplevelImageCaptureSourceManagerV1);
wayland_client::delegate_noop!(ClientState: ignore ExtForeignToplevelHandleV1);
wayland_client::delegate_noop!(ClientState: ignore ExtImageCaptureSourceV1);
wayland_client::delegate_noop!(ClientState: ignore ExtImageCopyCaptureManagerV1);
wayland_client::delegate_noop!(ClientState: ignore WlSeat);
wayland_client::delegate_noop!(ClientState: ignore WlPointer);

struct Setup {
    fixture: Fixture,
    id: ClientId,
    source_manager: ExtOutputImageCaptureSourceManagerV1,
    capture_manager: ExtImageCopyCaptureManagerV1,
    shm: WlShm,
}

fn setup() -> Setup {
    let mut config = Config::default();
    config.animations.off = true;
    config.layout.background_color = Color::from_rgba8_unpremul(RGBA[0], RGBA[1], RGBA[2], RGBA[3]);
    let mut fixture = Fixture::with_config(config);
    fixture.init_gles_renderer();
    fixture.add_output(1, (32, 24));
    let id = fixture.add_client();
    fixture.double_roundtrip(id);

    let c = fixture.client(id);
    let registry = c.display.get_registry(&c.qh, ());
    let global = |interface: &str| {
        c.state
            .globals
            .iter()
            .find(|g| g.interface == interface)
            .unwrap()
    };
    let source_global = global(ExtOutputImageCaptureSourceManagerV1::interface().name);
    let source_manager = registry.bind(source_global.name, 1, &c.qh, ());
    let capture_global = global(ExtImageCopyCaptureManagerV1::interface().name);
    let capture_manager = registry.bind(capture_global.name, 1, &c.qh, ());
    let shm_global = global(WlShm::interface().name);
    let shm = registry.bind(shm_global.name, 1, &c.qh, ());
    fixture.double_roundtrip(id);

    Setup {
        fixture,
        id,
        source_manager,
        capture_manager,
        shm,
    }
}

fn cursor_setup() -> (Setup, WlPointer) {
    let mut setup = setup();
    let pointer = {
        let c = setup.fixture.client(setup.id);
        let registry = c.display.get_registry(&c.qh, ());
        let seat_global = c
            .state
            .globals
            .iter()
            .find(|g| g.interface == WlSeat::interface().name)
            .unwrap();
        let seat: WlSeat = registry.bind(seat_global.name, 1, &c.qh, ());
        let pointer = seat.get_pointer(&c.qh, ());
        c.connection.flush().unwrap();
        pointer
    };
    setup.fixture.double_roundtrip(setup.id);
    (setup, pointer)
}

fn pump_until(f: &mut Fixture, id: ClientId, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(
            Instant::now() < deadline,
            "image copy capture did not finish within five seconds"
        );
        f.state.server.dispatch();
        f.client(id).dispatch();
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn create_session(setup: &mut Setup) -> (ExtImageCopyCaptureSessionV1, SessionEvents) {
    let c = setup.fixture.client(setup.id);
    let output = c.state.outputs.keys().next().unwrap().clone();
    let source = setup.source_manager.create_source(&output, &c.qh, ());
    let events = SessionEvents::default();
    let session =
        setup
            .capture_manager
            .create_session(&source, Options::empty(), &c.qh, events.clone());
    c.connection.flush().unwrap();
    pump_until(&mut setup.fixture, setup.id, || {
        let events = events.lock().unwrap();
        events.done || events.stopped
    });
    (session, events)
}

#[test]
fn output_source_captures_the_screen_capture_scene() {
    let mut setup = setup();
    let (session, session_events) = create_session(&mut setup);

    let (width, height) = {
        let events = session_events.lock().unwrap();
        assert!(!events.stopped, "output session must stay alive");
        assert_eq!(events.formats, [wl_shm::Format::Xrgb8888]);
        events.size.expect("buffer size must be advertised")
    };
    assert_eq!((width, height), (32, 24));

    let stride = width as usize * 4;
    let len = stride * height as usize;
    let path = temp_capture_file();
    let file = Arc::new(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap(),
    );
    std::fs::remove_file(path).unwrap();
    file.set_len(len as u64).unwrap();

    let frame_events = FrameEvents::default();
    {
        let c = setup.fixture.client(setup.id);
        let pool = setup.shm.create_pool(file.as_fd(), len as i32, &c.qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Xrgb8888,
            &c.qh,
            (),
        );
        pool.destroy();

        let frame = session.create_frame(&c.qh, frame_events.clone());
        frame.attach_buffer(&buffer);
        frame.damage_buffer(0, 0, width as i32, height as i32);
        frame.capture();
        c.connection.flush().unwrap();
    }

    // The headless fixture does not run device redraws; drive the same capture
    // entry point that the redraw path uses.
    let output = setup.fixture.clarion_output(1);
    let state = setup.fixture.clarion_state();
    state.clarion.update_render_elements(Some(&output));
    state
        .backend
        .with_renderer(|renderer| match renderer {
            crate::render_helpers::renderer::AnyRenderer::Gles(gles) => state
                .clarion
                .render_for_image_copy_capture(gles, &output, Duration::ZERO),
            crate::render_helpers::renderer::AnyRenderer::Native(native) => state
                .clarion
                .render_for_image_copy_capture(native, &output, Duration::ZERO),
        })
        .unwrap();

    pump_until(&mut setup.fixture, setup.id, || {
        frame_events.lock().unwrap().ready > 0
    });

    let events = frame_events.lock().unwrap();
    assert_eq!(events.ready, 1);
    assert_eq!(events.failed, 0);
    assert_eq!(
        events.transform,
        Some(wayland_client::protocol::wl_output::Transform::Normal)
    );
    assert_eq!(events.damage, [(0, 0, 32, 24)]);
    assert!(events.presentation_time.is_some());

    let mut bytes = vec![0; len];
    file.read_exact_at(&mut bytes, 0).unwrap();
    let expected: Vec<u8> = std::iter::repeat([RGBA[2], RGBA[1], RGBA[0], 255])
        .take((width * height) as usize)
        .flatten()
        .collect();
    assert_eq!(bytes, expected);
}

struct WindowSetup {
    fixture: Fixture,
    id: ClientId,
    capture_manager: ExtImageCopyCaptureManagerV1,
    toplevel_source_manager: ExtForeignToplevelImageCaptureSourceManagerV1,
    shm: WlShm,
    toplevels: Toplevels,
    surface: wayland_client::protocol::wl_surface::WlSurface,
    window: smithay::desktop::Window,
    window_id: u64,
}

fn window_setup() -> WindowSetup {
    let mut config = Config::default();
    config.animations.off = true;
    config.layout.border.off = true;
    config.layout.focus_ring.off = true;
    config.layout.shadow.on = false;
    config.hotkey_overlay.skip_at_startup = true;

    let mut fixture = Fixture::with_config(config);
    fixture.init_gles_renderer();
    fixture.add_output(1, (800, 600));
    let id = fixture.add_client();

    let window = fixture.client(id).create_window();
    let surface = window.surface.clone();
    window.commit();
    fixture.double_roundtrip(id);
    let window = fixture.client(id).window(&surface);
    let buffer = window
        .spbm
        .create_u32_rgba_buffer(u32::MAX, 0, 0, u32::MAX, &window.qh, ());
    window.surface.attach(Some(&buffer), 0, 0);
    window.set_size(320, 240);
    window.ack_last_and_commit();
    fixture.double_roundtrip(id);
    fixture.clarion_complete_animations();
    fixture.double_roundtrip(id);

    let window = fixture
        .clarion()
        .layout
        .active_window()
        .unwrap()
        .window
        .clone();
    let window_id = fixture.clarion().layout.active_window().unwrap().id().get();

    let toplevels = Toplevels::default();
    let (capture_manager, toplevel_source_manager, shm) = {
        let c = fixture.client(id);
        let registry = c.display.get_registry(&c.qh, ());
        let global = |interface: &str| {
            c.state
                .globals
                .iter()
                .find(|g| g.interface == interface)
                .unwrap()
        };
        let list_global = global(ExtForeignToplevelListV1::interface().name);
        registry.bind::<ExtForeignToplevelListV1, _, _>(
            list_global.name,
            1,
            &c.qh,
            toplevels.clone(),
        );
        let source_global = global(ExtForeignToplevelImageCaptureSourceManagerV1::interface().name);
        let toplevel_source_manager = registry.bind(source_global.name, 1, &c.qh, ());
        let capture_global = global(ExtImageCopyCaptureManagerV1::interface().name);
        let capture_manager = registry.bind(capture_global.name, 1, &c.qh, ());
        let shm_global = global(WlShm::interface().name);
        let shm = registry.bind(shm_global.name, 1, &c.qh, ());
        (capture_manager, toplevel_source_manager, shm)
    };
    fixture.double_roundtrip(id);
    assert_eq!(toplevels.lock().unwrap().len(), 1);

    WindowSetup {
        fixture,
        id,
        capture_manager,
        toplevel_source_manager,
        shm,
        toplevels,
        surface,
        window,
        window_id,
    }
}

fn create_window_session(setup: &mut WindowSetup) -> (ExtImageCopyCaptureSessionV1, SessionEvents) {
    let toplevel = setup.toplevels.lock().unwrap()[0].clone();
    let c = setup.fixture.client(setup.id);
    let source = setup
        .toplevel_source_manager
        .create_source(&toplevel, &c.qh, ());
    let events = SessionEvents::default();
    let session =
        setup
            .capture_manager
            .create_session(&source, Options::empty(), &c.qh, events.clone());
    c.connection.flush().unwrap();
    pump_until(&mut setup.fixture, setup.id, || {
        let events = events.lock().unwrap();
        events.done || events.stopped
    });
    (session, events)
}

fn temp_capture_file() -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "clarion-image-copy-{}-{unique}-{nanos}",
        std::process::id()
    ))
}

struct ShmCapture {
    file: Arc<std::fs::File>,
    buffer: WlBuffer,
    width: u32,
    height: u32,
    len: usize,
}

fn shm_capture(
    fixture: &mut Fixture,
    id: ClientId,
    shm: &WlShm,
    width: u32,
    height: u32,
    format: wl_shm::Format,
) -> ShmCapture {
    let stride = width as usize * 4;
    let len = stride * height as usize;
    let path = temp_capture_file();
    let file = Arc::new(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap(),
    );
    std::fs::remove_file(path).unwrap();
    file.set_len(len as u64).unwrap();

    let c = fixture.client(id);
    let pool = shm.create_pool(file.as_fd(), len as i32, &c.qh, ());
    let buffer = pool.create_buffer(
        0,
        width as i32,
        height as i32,
        stride as i32,
        format,
        &c.qh,
        (),
    );
    pool.destroy();
    c.connection.flush().unwrap();

    ShmCapture {
        file,
        buffer,
        width,
        height,
        len,
    }
}

fn request_frame(
    fixture: &mut Fixture,
    id: ClientId,
    session: &ExtImageCopyCaptureSessionV1,
    capture: &ShmCapture,
) -> (ExtImageCopyCaptureFrameV1, FrameEvents) {
    let events = FrameEvents::default();
    let c = fixture.client(id);
    let frame = session.create_frame(&c.qh, events.clone());
    frame.attach_buffer(&capture.buffer);
    frame.damage_buffer(0, 0, capture.width as i32, capture.height as i32);
    frame.capture();
    c.connection.flush().unwrap();
    fixture.state.server.dispatch();
    (frame, events)
}

fn drive_capture(fixture: &mut Fixture) {
    let output = fixture.clarion_output(1);
    let state = fixture.clarion_state();
    state.clarion.update_render_elements(Some(&output));
    state
        .backend
        .with_renderer(|renderer| match renderer {
            crate::render_helpers::renderer::AnyRenderer::Gles(gles) => {
                state
                    .clarion
                    .render_for_image_copy_capture(gles, &output, Duration::ZERO);
                state
                    .clarion
                    .render_windows_for_image_copy_capture(gles, &output, Duration::ZERO);
                state
                    .clarion
                    .render_for_image_copy_cursor_capture(gles, &output, Duration::ZERO);
            }
            crate::render_helpers::renderer::AnyRenderer::Native(native) => {
                state
                    .clarion
                    .render_for_image_copy_capture(native, &output, Duration::ZERO);
                state.clarion.render_windows_for_image_copy_capture(
                    native,
                    &output,
                    Duration::ZERO,
                );
                state
                    .clarion
                    .render_for_image_copy_cursor_capture(native, &output, Duration::ZERO);
            }
        })
        .unwrap();
}

fn pump_frame(fixture: &mut Fixture, id: ClientId, events: &FrameEvents) {
    pump_until(fixture, id, || {
        let events = events.lock().unwrap();
        events.ready > 0 || events.failed > 0
    });
}

#[test]
fn toplevel_source_captures_window_pixels() {
    let mut setup = window_setup();
    let (session, session_events) = create_window_session(&mut setup);

    let (width, height) = {
        let events = session_events.lock().unwrap();
        assert!(!events.stopped, "window session must stay alive");
        assert!(events.formats.contains(&wl_shm::Format::Xrgb8888));
        assert!(events.formats.contains(&wl_shm::Format::Argb8888));
        events.size.expect("buffer size must be advertised")
    };
    assert_eq!((width, height), (320, 240));

    let capture = shm_capture(
        &mut setup.fixture,
        setup.id,
        &setup.shm,
        width,
        height,
        wl_shm::Format::Xrgb8888,
    );
    let (frame, frame_events) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    drive_capture(&mut setup.fixture);
    pump_frame(&mut setup.fixture, setup.id, &frame_events);

    let events = frame_events.lock().unwrap();
    assert_eq!(events.ready, 1);
    assert_eq!(events.failed, 0);
    assert_eq!(
        events.transform,
        Some(wayland_client::protocol::wl_output::Transform::Normal)
    );
    assert_eq!(events.damage, [(0, 0, 320, 240)]);

    let mut bytes = vec![0; capture.len];
    capture.file.read_exact_at(&mut bytes, 0).unwrap();
    assert!(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [0, 0, 255, 255]),
        "window capture did not contain the window content"
    );

    drop(events);
    frame.destroy();
}

#[test]
fn minimized_window_pauses_capture_and_resumes() {
    let mut setup = window_setup();
    let (session, session_events) = create_window_session(&mut setup);
    let (width, height) = session_events.lock().unwrap().size.unwrap();

    let capture = shm_capture(
        &mut setup.fixture,
        setup.id,
        &setup.shm,
        width,
        height,
        wl_shm::Format::Xrgb8888,
    );
    let (first, first_events) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    drive_capture(&mut setup.fixture);
    pump_frame(&mut setup.fixture, setup.id, &first_events);
    assert_eq!(first_events.lock().unwrap().ready, 1);
    first.destroy();

    // With no new window damage, the next frame waits until minimization fails it.
    let (second, second_events) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    drive_capture(&mut setup.fixture);
    assert_eq!(second_events.lock().unwrap().ready, 0);
    assert_eq!(second_events.lock().unwrap().failed, 0);
    assert!(setup.fixture.clarion().image_copy_sessions[0]
        .pending_frame
        .is_some());

    setup
        .fixture
        .client(setup.id)
        .window(&setup.surface)
        .xdg_toplevel
        .set_minimized();
    setup.fixture.double_roundtrip(setup.id);
    setup.fixture.clarion_complete_animations();
    assert!(setup.fixture.clarion().layout.is_minimized(&setup.window));

    drive_capture(&mut setup.fixture);
    pump_frame(&mut setup.fixture, setup.id, &second_events);
    assert_eq!(second_events.lock().unwrap().ready, 0);
    assert_eq!(second_events.lock().unwrap().failed, 1);
    assert_eq!(
        second_events.lock().unwrap().failure_reason,
        Some(WEnum::Value(
            ext_image_copy_capture_frame_v1::FailureReason::Unknown
        ))
    );
    assert!(!session_events.lock().unwrap().stopped);
    assert_eq!(setup.fixture.clarion().image_copy_sessions.len(), 1);
    assert!(setup.fixture.clarion().image_copy_sessions[0]
        .pending_frame
        .is_none());
    second.destroy();

    assert!(setup.fixture.clarion().layout.restore_window(&setup.window));
    setup.fixture.double_roundtrip(setup.id);
    setup.fixture.clarion_complete_animations();
    assert!(!setup.fixture.clarion().layout.is_minimized(&setup.window));

    // The existing session retains damage history; a fresh commit resumes delivery.
    let window = setup.fixture.client(setup.id).window(&setup.surface);
    let buffer = window
        .spbm
        .create_u32_rgba_buffer(u32::MAX, 0, 0, u32::MAX, &window.qh, ());
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.damage_buffer(0, 0, 1, 1);
    window.ack_last_and_commit();
    setup.fixture.double_roundtrip(setup.id);

    let (restored, restored_events) =
        request_frame(&mut setup.fixture, setup.id, &session, &capture);
    drive_capture(&mut setup.fixture);
    pump_frame(&mut setup.fixture, setup.id, &restored_events);
    assert_eq!(restored_events.lock().unwrap().failed, 0);
    assert_eq!(restored_events.lock().unwrap().ready, 1);
    assert!(!session_events.lock().unwrap().stopped);
    restored.destroy();
}

#[test]
fn window_capture_respects_block_out_from_screen_capture() {
    let mut setup = window_setup();
    let rule = WindowRule {
        block_out_from: Some(BlockOutFrom::ScreenCapture),
        ..Default::default()
    };
    setup
        .fixture
        .clarion()
        .config
        .borrow_mut()
        .window_rules
        .push(rule.clone());
    setup
        .fixture
        .clarion()
        .layout
        .with_windows_mut(|mapped, _| {
            mapped.recompute_window_rules(std::slice::from_ref(&rule), false);
        });
    setup
        .fixture
        .clarion()
        .layout
        .update_window(&setup.window, None);

    let (session, session_events) = create_window_session(&mut setup);
    let (width, height) = session_events.lock().unwrap().size.unwrap();
    let capture = shm_capture(
        &mut setup.fixture,
        setup.id,
        &setup.shm,
        width,
        height,
        wl_shm::Format::Xrgb8888,
    );
    let (frame, frame_events) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    drive_capture(&mut setup.fixture);
    pump_frame(&mut setup.fixture, setup.id, &frame_events);
    assert_eq!(frame_events.lock().unwrap().ready, 1);

    let mut bytes = vec![0; capture.len];
    capture.file.read_exact_at(&mut bytes, 0).unwrap();
    assert!(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[0..3] == [0, 0, 0]),
        "block-out-from screen-capture leaked window contents"
    );
    frame.destroy();
}

fn move_pointer(f: &mut Fixture, pos: (f64, f64)) {
    let pos = smithay::utils::Point::from(pos);
    let under = f.clarion().contents_under(pos);
    let pointer = f.clarion().seat.get_pointer().unwrap();
    pointer.motion(
        f.clarion_state(),
        under.surface,
        &smithay::input::pointer::MotionEvent {
            location: pos,
            serial: smithay::utils::SERIAL_COUNTER.next_serial(),
            time: smithay::backend::input::InputTime::from_millis(1),
        },
    );
    pointer.frame(f.clarion_state());
    f.clarion().refresh_pointer_outputs();
}

#[test]
fn cursor_session_captures_cursor_and_updates_position() {
    let (mut setup, pointer) = cursor_setup();
    start_ipc(&mut setup.fixture);

    let output = {
        let c = setup.fixture.client(setup.id);
        c.state.outputs.keys().next().unwrap().clone()
    };
    let cursor_events = CursorEvents::default();
    let session_events = SessionEvents::default();
    let (cursor_session, session) = {
        let c = setup.fixture.client(setup.id);
        let source = setup.source_manager.create_source(&output, &c.qh, ());
        let cursor_session = setup.capture_manager.create_pointer_cursor_session(
            &source,
            &pointer,
            &c.qh,
            cursor_events.clone(),
        );
        let session = cursor_session.get_capture_session(&c.qh, session_events.clone());
        c.connection.flush().unwrap();
        (cursor_session, session)
    };
    pump_until(&mut setup.fixture, setup.id, || {
        let events = session_events.lock().unwrap();
        events.done || events.stopped
    });

    let (width, height) = session_events
        .lock()
        .unwrap()
        .size
        .expect("cursor buffer size must be advertised");
    assert!(
        width > 1 && height > 1,
        "cursor size must be non-degenerate"
    );
    {
        let events = cursor_events.lock().unwrap();
        assert!(!session_events.lock().unwrap().stopped);
        assert_eq!(events.enter, 1, "cursor must enter the output");
        assert_eq!(events.position, Some((0, 0)));
    }

    // The cursor image can still be loading when the session starts, so the
    // first frame may fail with BufferConstraints. Follow the protocol and
    // retry with the renegotiated size.
    let (_frame, frame_events, capture, width, height) = 'attempts: {
        for _ in 0..4 {
            let previous_done = session_events.lock().unwrap().done_events;
            let (width, height) = session_events.lock().unwrap().size.unwrap();
            let capture = shm_capture(
                &mut setup.fixture,
                setup.id,
                &setup.shm,
                width,
                height,
                wl_shm::Format::Argb8888,
            );
            let (frame, frame_events) =
                request_frame(&mut setup.fixture, setup.id, &session, &capture);
            drive_capture(&mut setup.fixture);
            pump_frame(&mut setup.fixture, setup.id, &frame_events);
            let ready = {
                let events = frame_events.lock().unwrap();
                events.ready
            };
            if ready > 0 {
                break 'attempts (frame, frame_events, capture, width, height);
            }
            assert_eq!(
                frame_events.lock().unwrap().failed,
                1,
                "cursor frame failed unexpectedly"
            );
            frame.destroy();
            // Wait for the new constraints.
            pump_until(&mut setup.fixture, setup.id, || {
                session_events.lock().unwrap().done_events > previous_done
            });
        }
        panic!("cursor constraints kept changing");
    };

    {
        let events = frame_events.lock().unwrap();
        assert_eq!(events.ready, 1);
        assert_eq!(events.failed, 0);
        assert_eq!(events.damage, [(0, 0, width as i32, height as i32)]);
    }
    let mut bytes = vec![0; capture.len];
    capture.file.read_exact_at(&mut bytes, 0).unwrap();
    assert!(
        bytes.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0),
        "cursor capture is empty"
    );
    let cursor_state = cursor_events.lock().unwrap();
    assert!(
        cursor_state.hotspot.is_some(),
        "cursor hotspot must be sent"
    );
    drop(cursor_state);
    _frame.destroy();

    // Requesting another frame makes the session a cast.
    let extra_capture = shm_capture(
        &mut setup.fixture,
        setup.id,
        &setup.shm,
        width,
        height,
        wl_shm::Format::Argb8888,
    );
    let (extra, _) = request_frame(&mut setup.fixture, setup.id, &session, &extra_capture);
    let casts = casts(&mut setup.fixture);
    assert_eq!(casts.len(), 1);
    assert_eq!(casts[0].kind, CastKind::ExtImageCopyCaptureCursor);
    assert!(casts[0].is_active);
    extra.destroy();

    // Moving the pointer outside the output makes the cursor leave, clearing
    // its position.
    move_pointer(&mut setup.fixture, (100., 100.));
    setup.fixture.clarion().refresh_image_copy_cursor_sessions();
    setup.fixture.double_roundtrip(setup.id);
    let events = cursor_events.lock().unwrap();
    assert!(events.leave >= 1, "cursor must leave the output");
    drop(events);

    cursor_session.destroy();
    session.destroy();
}

fn start_ipc(f: &mut Fixture) {
    let handle = f.clarion().event_loop.clone();
    let server = crate::ipc::server::IpcServer::start(&handle, None).unwrap();
    f.clarion().ipc_server = Some(server);
    // Seed the state that the normal State constructor seeds for the original
    // socket-less test server.
    f.clarion_state().ipc_keyboard_layouts_changed();
}

fn casts(f: &mut Fixture) -> Vec<Cast> {
    f.clarion_state().ipc_refresh_casts();
    f.clarion().ipc_server.as_ref().unwrap().test_casts()
}

#[test]
fn image_copy_cast_is_reported_from_the_second_frame() {
    let mut setup = setup();
    start_ipc(&mut setup.fixture);
    let (session, session_events) = create_session(&mut setup);
    let (width, height) = session_events.lock().unwrap().size.unwrap();
    let capture = shm_capture(
        &mut setup.fixture,
        setup.id,
        &setup.shm,
        width,
        height,
        wl_shm::Format::Xrgb8888,
    );

    let (first, first_events) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    drive_capture(&mut setup.fixture);
    pump_frame(&mut setup.fixture, setup.id, &first_events);
    assert_eq!(first_events.lock().unwrap().ready, 1);
    assert!(
        casts(&mut setup.fixture).is_empty(),
        "a single frame must not be reported as a cast"
    );
    first.destroy();

    // The second request makes the session a cast even before it completes.
    let (second, _) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    let casts = casts(&mut setup.fixture);
    assert_eq!(casts.len(), 1);
    assert_eq!(casts[0].kind, CastKind::ExtImageCopyCapture);
    assert!(casts[0].is_active);
    assert_eq!(
        casts[0].target,
        CastTarget::Output {
            name: setup.fixture.clarion_output(1).name(),
        }
    );
    second.destroy();
    session.destroy();
}

#[test]
fn minimized_window_image_copy_cast_is_inactive() {
    let mut setup = window_setup();
    start_ipc(&mut setup.fixture);
    let (session, session_events) = create_window_session(&mut setup);
    let (width, height) = session_events.lock().unwrap().size.unwrap();
    let capture = shm_capture(
        &mut setup.fixture,
        setup.id,
        &setup.shm,
        width,
        height,
        wl_shm::Format::Xrgb8888,
    );

    let (first, first_events) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    drive_capture(&mut setup.fixture);
    pump_frame(&mut setup.fixture, setup.id, &first_events);
    assert_eq!(first_events.lock().unwrap().ready, 1);
    first.destroy();

    let (second, _) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    let before = casts(&mut setup.fixture);
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].kind, CastKind::ExtImageCopyCapture);
    assert!(before[0].is_active);
    assert_eq!(
        before[0].target,
        CastTarget::Window {
            id: setup.window_id,
        }
    );

    setup
        .fixture
        .clarion()
        .refresh_image_copy_cast_window_rules();
    let mapped = setup
        .fixture
        .clarion()
        .layout
        .windows()
        .find(|(_, mapped)| mapped.id().get() == setup.window_id)
        .unwrap()
        .1;
    assert!(
        mapped.is_window_cast_target(),
        "an ext-image-copy window session must set is-window-cast-target"
    );

    setup
        .fixture
        .client(setup.id)
        .window(&setup.surface)
        .xdg_toplevel
        .set_minimized();
    setup.fixture.double_roundtrip(setup.id);
    setup.fixture.clarion_complete_animations();

    let after = casts(&mut setup.fixture);
    assert_eq!(after.len(), 1);
    assert!(!after[0].is_active, "a minimized window cast is inactive");
    assert_eq!(after[0].target, before[0].target);

    second.destroy();
    session.destroy();
    setup.fixture.double_roundtrip(setup.id);
    assert!(casts(&mut setup.fixture).is_empty());

    let mapped = setup
        .fixture
        .clarion()
        .layout
        .windows()
        .find(|(_, mapped)| mapped.id().get() == setup.window_id)
        .unwrap()
        .1;
    assert!(
        !mapped.is_window_cast_target(),
        "is-window-cast-target must clear when the cast stops"
    );
}

#[test]
fn window_resize_renegotiates_image_copy_constraints() {
    let mut setup = window_setup();
    let (session, session_events) = create_window_session(&mut setup);
    let (width, height) = session_events.lock().unwrap().size.unwrap();
    let capture = shm_capture(
        &mut setup.fixture,
        setup.id,
        &setup.shm,
        width,
        height,
        wl_shm::Format::Xrgb8888,
    );

    let (first, first_events) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    drive_capture(&mut setup.fixture);
    pump_frame(&mut setup.fixture, setup.id, &first_events);
    assert_eq!(first_events.lock().unwrap().ready, 1);
    first.destroy();

    // The next frame waits for damage, so the resize fails it with new
    // constraints.
    let (second, second_events) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    assert!(setup.fixture.clarion().image_copy_sessions[0]
        .pending_frame
        .is_some());

    let window = setup.fixture.client(setup.id).window(&setup.surface);
    let buffer = window
        .spbm
        .create_u32_rgba_buffer(u32::MAX, 0, 0, u32::MAX, &window.qh, ());
    window.surface.attach(Some(&buffer), 0, 0);
    window.set_size(200, 150);
    window.ack_last_and_commit();

    let previous_done = session_events.lock().unwrap().done_events;
    setup.fixture.double_roundtrip(setup.id);
    pump_until(&mut setup.fixture, setup.id, || {
        session_events.lock().unwrap().done_events > previous_done
    });
    assert_eq!(session_events.lock().unwrap().size, Some((200, 150)));
    assert_eq!(second_events.lock().unwrap().failed, 1);
    assert_eq!(
        second_events.lock().unwrap().failure_reason,
        Some(WEnum::Value(
            ext_image_copy_capture_frame_v1::FailureReason::BufferConstraints
        ))
    );
    assert_eq!(second_events.lock().unwrap().ready, 0);
    assert!(!session_events.lock().unwrap().stopped);
    second.destroy();

    let resized = shm_capture(
        &mut setup.fixture,
        setup.id,
        &setup.shm,
        200,
        150,
        wl_shm::Format::Xrgb8888,
    );
    let (third, third_events) = request_frame(&mut setup.fixture, setup.id, &session, &resized);
    drive_capture(&mut setup.fixture);
    pump_frame(&mut setup.fixture, setup.id, &third_events);
    assert_eq!(third_events.lock().unwrap().ready, 1);
    assert_eq!(third_events.lock().unwrap().failed, 0);
    third.destroy();
}

#[test]
fn closing_a_window_drops_its_image_copy_session() {
    let mut setup = window_setup();
    let (session, session_events) = create_window_session(&mut setup);
    let (width, height) = session_events.lock().unwrap().size.unwrap();
    let capture = shm_capture(
        &mut setup.fixture,
        setup.id,
        &setup.shm,
        width,
        height,
        wl_shm::Format::Xrgb8888,
    );

    let (first, first_events) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    drive_capture(&mut setup.fixture);
    pump_frame(&mut setup.fixture, setup.id, &first_events);
    assert_eq!(first_events.lock().unwrap().ready, 1);
    first.destroy();

    let (second, second_events) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    assert!(setup.fixture.clarion().image_copy_sessions[0]
        .pending_frame
        .is_some());

    let window = setup.fixture.client(setup.id).window(&setup.surface);
    window.surface.attach(None, 0, 0);
    window.commit();
    setup.fixture.double_roundtrip(setup.id);

    assert!(session_events.lock().unwrap().stopped);
    assert_eq!(second_events.lock().unwrap().failed, 1);
    assert!(setup.fixture.clarion().image_copy_sessions.is_empty());
    second.destroy();
    session.destroy();
}

#[test]
fn oversized_capture_buffer_fails_with_buffer_constraints() {
    let mut setup = window_setup();
    let (session, session_events) = create_window_session(&mut setup);
    let (width, height) = session_events.lock().unwrap().size.unwrap();
    let capture = shm_capture(
        &mut setup.fixture,
        setup.id,
        &setup.shm,
        width + 16,
        height + 16,
        wl_shm::Format::Xrgb8888,
    );
    let (frame, frame_events) = request_frame(&mut setup.fixture, setup.id, &session, &capture);
    drive_capture(&mut setup.fixture);
    pump_frame(&mut setup.fixture, setup.id, &frame_events);

    let events = frame_events.lock().unwrap();
    assert_eq!(events.ready, 0);
    assert_eq!(events.failed, 1);
    assert_eq!(
        events.failure_reason,
        Some(WEnum::Value(
            ext_image_copy_capture_frame_v1::FailureReason::BufferConstraints
        ))
    );
    drop(events);
    assert!(setup.fixture.clarion().image_copy_sessions[0]
        .pending_frame
        .is_none());
    frame.destroy();
}
