use clarion_config::Action;
use clarion_ipc::state::EventStreamStatePart as _;

use super::Fixture;

#[test]
fn toggle_touchpad_flips_runtime_state_and_reports_event() {
    let mut f = Fixture::new();
    let handle = f.clarion().event_loop.clone();
    let server = crate::ipc::server::IpcServer::start(&handle, None).unwrap();
    f.clarion().ipc_server = Some(server);
    f.clarion_state().ipc_keyboard_layouts_changed();
    f.clarion_state().ipc_refresh_touchpad();

    assert!(
        f.clarion().ipc_server.as_ref().unwrap().test_touchpad_on(),
        "touchpad starts enabled"
    );

    f.clarion_state().do_action(Action::ToggleTouchpad, false);
    assert!(f.clarion().touchpad_disabled_by_toggle);
    f.clarion_state().ipc_refresh_touchpad();
    assert!(
        !f.clarion().ipc_server.as_ref().unwrap().test_touchpad_on(),
        "toggle off must be reported"
    );

    f.clarion_state().do_action(Action::ToggleTouchpad, false);
    assert!(!f.clarion().touchpad_disabled_by_toggle);
    f.clarion_state().ipc_refresh_touchpad();
    assert!(
        f.clarion().ipc_server.as_ref().unwrap().test_touchpad_on(),
        "toggle on must be reported"
    );
}

#[test]
fn touchpad_toggled_state_roundtrips_through_event_stream() {
    let mut state = clarion_ipc::state::TouchpadState::default();
    assert!(state
        .apply(clarion_ipc::Event::TouchpadToggled { on: true })
        .is_none());
    assert!(state.on);
    assert!(matches!(
        state.replicate().as_slice(),
        [clarion_ipc::Event::TouchpadToggled { on: true }]
    ));
    assert!(state
        .apply(clarion_ipc::Event::TouchpadToggled { on: false })
        .is_none());
    assert!(!state.on);
}
