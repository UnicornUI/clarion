use super::*;

fn stage_manager_layout(on: bool, windows: usize) -> Layout<TestWindow> {
    let mut options = Options::default();
    options.overview.stage_manager.on = on;
    let mut ops = vec![Op::AddScaledOutput {
        id: 1,
        scale: 1.,
        layout_config: None,
    }];
    for id in 1..=windows {
        ops.push(Op::AddWindow {
            params: TestWindowParams::new(id),
        });
    }
    ops.push(Op::CompleteAnimations);
    check_ops_with_options(options, ops)
}

fn settle(layout: &mut Layout<TestWindow>, ids: &[usize]) {
    layout.update_render_elements(None);
    let mut ops: Vec<Op> = ids.iter().map(|id| Op::Communicate(*id)).collect();
    ops.push(Op::CompleteAnimations);
    check_ops_on_layout(layout, ops);
}

fn active(layout: &Layout<TestWindow>) -> usize {
    *layout
        .active_monitor_ref()
        .unwrap()
        .active_workspace_ref()
        .active_window()
        .unwrap()
        .id()
}

fn hidden(layout: &Layout<TestWindow>) -> Vec<u64> {
    let mut ids = layout
        .active_monitor_ref()
        .unwrap()
        .stage_hidden()
        .to_vec();
    ids.sort_unstable();
    ids
}

fn rail(layout: &Layout<TestWindow>) -> Vec<usize> {
    let output = layout.outputs().next().unwrap().clone();
    let mut ids: Vec<_> = layout
        .stage_manager_windows(&output)
        .iter()
        .map(|window| *window.id())
        .collect();
    ids.sort_unstable();
    ids
}

#[test]
fn stage_manager_collects_every_other_window() {
    let mut layout = stage_manager_layout(true, 3);
    settle(&mut layout, &[1, 2, 3]);

    assert_eq!(active(&layout), 3);
    assert_eq!(rail(&layout), vec![1, 2], "every other window is in the rail");
    let mon = layout.active_monitor_ref().unwrap();
    let active_core = mon.active_workspace_ref().active_window().unwrap().core_id();
    let ids: Vec<u64> = hidden(&layout);
    assert_eq!(ids.len(), 2);
    assert!(!ids.contains(&active_core));
    for id in [1, 2, 3] {
        assert!(!layout.is_minimized(&id), "window {id} must not be minimized");
    }
    // Presentation only: the stage window keeps its layout geometry.
    let rect = mon
        .active_workspace_ref()
        .window_render_rect(&3)
        .unwrap();
    assert!(rect.size.w > 0. && rect.size.h > 0.);
}

#[test]
fn stage_manager_follows_focus_and_keeps_every_window() {
    let mut layout = stage_manager_layout(true, 4);
    settle(&mut layout, &[1, 2, 3, 4]);
    assert_eq!(rail(&layout), vec![1, 2, 3]);

    layout.activate_window(&1);
    settle(&mut layout, &[1, 2, 3, 4]);
    assert_eq!(active(&layout), 1);
    assert_eq!(rail(&layout), vec![2, 3, 4]);
    for id in [1, 2, 3, 4] {
        assert!(!layout.is_minimized(&id));
    }

    layout.activate_window(&4);
    settle(&mut layout, &[1, 2, 3, 4]);
    assert_eq!(active(&layout), 4);
    assert_eq!(rail(&layout), vec![1, 2, 3]);
}

#[test]
fn stage_manager_off_leaves_every_window_alone() {
    let mut layout = stage_manager_layout(false, 3);
    settle(&mut layout, &[1, 2, 3]);
    assert!(rail(&layout).is_empty());
    assert!(hidden(&layout).is_empty());
    for id in [1, 2, 3] {
        assert!(!layout.is_minimized(&id));
    }
}

#[test]
fn stage_manager_releases_the_stage_for_overview_and_spread() {
    let mut layout = stage_manager_layout(true, 3);
    settle(&mut layout, &[1, 2, 3]);
    assert_eq!(hidden(&layout).len(), 2);

    layout.toggle_overview();
    layout.update_render_elements(None);
    assert!(hidden(&layout).is_empty(), "the overview shows every window");
    assert!(rail(&layout).is_empty(), "no rail while the overview is open");

    layout.toggle_overview();
    layout.clock.set_complete_instantly(true);
    layout.advance_animations();
    layout.clock.set_complete_instantly(false);
    settle(&mut layout, &[1, 2, 3]);
    assert_eq!(hidden(&layout).len(), 2);
    assert_eq!(rail(&layout), vec![1, 2]);
}

#[test]
fn stage_manager_clicked_window_takes_the_stage() {
    let mut layout = stage_manager_layout(true, 3);
    settle(&mut layout, &[1, 2, 3]);
    assert_eq!(active(&layout), 3);

    // Exactly what the rail click handler does.
    layout.suspend_minimize_animations(true);
    layout.activate_window(&1);
    layout.suspend_minimize_animations(false);
    settle(&mut layout, &[1, 2, 3]);

    assert_eq!(active(&layout), 1, "the clicked window takes the stage");
    assert_eq!(rail(&layout), vec![2, 3]);
    let mon = layout.active_monitor_ref().unwrap();
    let active_core = mon.active_workspace_ref().active_window().unwrap().core_id();
    assert!(!hidden(&layout).contains(&active_core));
}

#[test]
fn stage_manager_collects_dwindle_windows() {
    let mut options = Options::default();
    options.overview.stage_manager.on = true;
    options.layout.auto_stack_new_windows = false;
    let mut ops = vec![
        Op::AddScaledOutput {
            id: 1,
            scale: 1.,
            layout_config: None,
        },
        Op::SwitchTilingMode(Some(clarion_ipc::TilingMode::Dwindle)),
    ];
    for id in 1..=3 {
        ops.push(Op::AddWindow {
            params: TestWindowParams::new(id),
        });
    }
    ops.push(Op::CompleteAnimations);
    let mut layout = check_ops_with_options(options, ops);
    settle(&mut layout, &[1, 2, 3]);

    let mon = layout.active_monitor_ref().unwrap();
    let active = mon.active_workspace_ref().active_window().unwrap().core_id();
    let hidden = mon.stage_hidden();
    assert_eq!(hidden.len(), 2, "every other dwindle pane is collected");
    assert!(!hidden.contains(&active));
    assert_eq!(rail(&layout), vec![1, 2]);
}

#[test]
fn stage_manager_keeps_a_floating_window_at_its_own_size() {
    let mut options = Options::default();
    options.overview.stage_manager.on = true;
    let mut ops = vec![Op::AddScaledOutput {
        id: 1,
        scale: 1.,
        layout_config: None,
    }];
    for id in 1..=3 {
        let mut params = TestWindowParams::new(id);
        params.is_floating = true;
        ops.push(Op::AddWindow { params });
    }
    ops.push(Op::CompleteAnimations);
    let mut layout = check_ops_with_options(options, ops);
    settle(&mut layout, &[1, 2, 3]);

    let mon = layout.active_monitor_ref().unwrap();
    let area = mon.working_area();
    let active = *mon.active_workspace_ref().active_window().unwrap().id();
    let rect = mon
        .active_workspace_ref()
        .window_render_rect(&active)
        .unwrap();
    assert!(
        rect.size.w < area.size.w * 0.9,
        "the floating window keeps its own size: {rect:?}"
    );
    assert_eq!(mon.stage_hidden().len(), 2);
}
