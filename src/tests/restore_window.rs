use clarion_config::Action;
use smithay::desktop::Window;

use super::client::ClientId;
use super::Fixture;

fn fixture() -> Fixture {
    let mut config = clarion_config::Config::default();
    config.animations.off = true;
    config.hotkey_overlay.skip_at_startup = true;
    let mut f = Fixture::with_config(config);
    f.add_output(1, (1280, 800));
    f
}

fn add_window(
    f: &mut Fixture,
) -> (
    ClientId,
    wayland_client::protocol::wl_surface::WlSurface,
    Window,
) {
    let client = f.add_client();
    let w = f.client(client).create_window();
    let surface = w.surface.clone();
    w.commit();
    f.double_roundtrip(client);
    let w = f.client(client).window(&surface);
    let buffer = w.spbm.create_u32_rgba_buffer(0, 0, 0, u32::MAX, &w.qh, ());
    w.surface.attach(Some(&buffer), 0, 0);
    w.set_size(320, 240);
    w.ack_last_and_commit();
    f.double_roundtrip(client);
    f.clarion_complete_animations();
    let window = f.clarion().layout.active_window().unwrap().window.clone();
    (client, surface, window)
}

#[test]
fn restore_window_restores_most_recent_or_by_id() {
    let mut f = fixture();
    let (_client, _surface, window) = add_window(&mut f);
    let id = f
        .clarion()
        .layout
        .windows()
        .find(|(_, mapped)| mapped.window == window)
        .map(|(_, mapped)| mapped.id().get())
        .unwrap();

    f.clarion_state()
        .do_action(Action::MinimizeWindowById(id), false);
    assert!(f.clarion().layout.is_minimized(&window));

    f.clarion_state()
        .do_action(Action::RestoreWindow(None, None), false);
    assert!(!f.clarion().layout.is_minimized(&window));
    assert_eq!(
        f.clarion().layout.focus().map(|mapped| &mapped.window),
        Some(&window)
    );

    f.clarion_state()
        .do_action(Action::MinimizeWindowById(id), false);
    assert!(f.clarion().layout.is_minimized(&window));

    f.clarion_state()
        .do_action(Action::RestoreWindow(Some(id), None), false);
    assert!(!f.clarion().layout.is_minimized(&window));
    assert_eq!(
        f.clarion().layout.focus().map(|mapped| &mapped.window),
        Some(&window)
    );
}
