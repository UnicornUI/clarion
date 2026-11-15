use clarion_config::{Action, Config};

use super::Fixture;
use crate::layout::{LayoutElement as _, SizingMode};

fn setup() -> (Fixture, u64) {
    let mut config = Config::default();
    config.hotkey_overlay.skip_at_startup = true;
    config.animations.window_open.anim.off = true;
    config.animations.window_close.anim.off = true;
    let mut f = Fixture::with_config(config);
    f.add_output(1, (1280, 800));

    let client = f.add_client();
    let window = f.client(client).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.double_roundtrip(client);
    let window = f.client(client).window(&surface);
    let buffer = window
        .spbm
        .create_u32_rgba_buffer(u32::MAX, 0, 0, u32::MAX, &window.qh, ());
    window.surface.attach(Some(&buffer), 0, 0);
    window.set_size(320, 240);
    window.ack_last_and_commit();
    f.double_roundtrip(client);
    f.clarion_complete_animations();

    let id = f.clarion().layout.active_window().unwrap().id().get();
    (f, id)
}

fn pending_mode(f: &mut Fixture, id: u64) -> SizingMode {
    f.clarion()
        .layout
        .windows()
        .find(|(_, window)| window.id().get() == id)
        .unwrap()
        .1
        .pending_sizing_mode()
}

fn pending_windowed_fullscreen(f: &mut Fixture, id: u64) -> bool {
    f.clarion()
        .layout
        .windows()
        .find(|(_, window)| window.id().get() == id)
        .unwrap()
        .1
        .is_pending_windowed_fullscreen()
}

#[test]
fn set_and_unset_window_state_actions_are_not_toggles() {
    let (mut f, id) = setup();

    f.clarion_state()
        .do_action(Action::SetWindowFullscreenById(id), false);
    f.clarion_complete_animations();
    assert_eq!(pending_mode(&mut f, id), SizingMode::Fullscreen);

    // A repeated set must keep the requested state, not toggle it off.
    f.clarion_state()
        .do_action(Action::SetWindowFullscreenById(id), false);
    f.clarion_complete_animations();
    assert_eq!(pending_mode(&mut f, id), SizingMode::Fullscreen);

    f.clarion_state()
        .do_action(Action::UnsetWindowFullscreenById(id), false);
    f.clarion_complete_animations();
    assert_eq!(pending_mode(&mut f, id), SizingMode::Normal);

    f.clarion_state()
        .do_action(Action::SetWindowMaximizedToEdgesById(id), false);
    f.clarion_complete_animations();
    assert_eq!(pending_mode(&mut f, id), SizingMode::Maximized);

    f.clarion_state()
        .do_action(Action::SetWindowMaximizedToEdgesById(id), false);
    f.clarion_complete_animations();
    assert_eq!(pending_mode(&mut f, id), SizingMode::Maximized);

    f.clarion_state()
        .do_action(Action::UnsetWindowMaximizedToEdgesById(id), false);
    f.clarion_complete_animations();
    assert_eq!(pending_mode(&mut f, id), SizingMode::Normal);

    f.clarion_state()
        .do_action(Action::SetWindowedFullscreenById(id), false);
    assert!(pending_windowed_fullscreen(&mut f, id));

    f.clarion_state()
        .do_action(Action::SetWindowedFullscreenById(id), false);
    assert!(pending_windowed_fullscreen(&mut f, id));

    f.clarion_state()
        .do_action(Action::UnsetWindowedFullscreenById(id), false);
    assert!(!pending_windowed_fullscreen(&mut f, id));
}
