//! End-to-end ext-image-copy-capture output tests using a real Wayland client.

use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clarion_config::{Color, Config};
use smithay::reexports::wayland_protocols::ext::image_capture_source::v1::client::ext_image_capture_source_v1::ExtImageCaptureSourceV1;
use smithay::reexports::wayland_protocols::ext::image_capture_source::v1::client::ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1;
use smithay::reexports::wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::{
    self, ExtImageCopyCaptureFrameV1,
};
use smithay::reexports::wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::{
    ExtImageCopyCaptureManagerV1, Options,
};
use smithay::reexports::wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_session_v1::{
    self, ExtImageCopyCaptureSessionV1,
};
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
            ext_image_copy_capture_session_v1::Event::Done => events.done = true,
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
            ext_image_copy_capture_frame_v1::Event::Failed { .. } => events.failed += 1,
            _ => unreachable!(),
        }
    }
}

wayland_client::delegate_noop!(ClientState: ignore ExtOutputImageCaptureSourceManagerV1);
wayland_client::delegate_noop!(ClientState: ignore ExtImageCaptureSourceV1);
wayland_client::delegate_noop!(ClientState: ignore ExtImageCopyCaptureManagerV1);

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
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "clarion-image-copy-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
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
