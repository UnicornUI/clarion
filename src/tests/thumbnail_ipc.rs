use base64::Engine as _;
use clarion_config::{Config, Workspace, WorkspaceName};

use super::client::ClientId;
use super::Fixture;
use crate::layout::LayoutElement as _;

fn add_window(f: &mut Fixture, client: ClientId, color: [u32; 4]) {
    let window = f.client(client).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.double_roundtrip(client);
    let window = f.client(client).window(&surface);
    let buffer = window.spbm.create_u32_rgba_buffer(
        color[0], color[1], color[2], color[3], &window.qh, (),
    );
    window.surface.attach(Some(&buffer), 0, 0);
    window.set_size(1100, 700);
    window.ack_last_and_commit();
    f.double_roundtrip(client);
}

#[test]
fn the_gpu_thumbnail_reads_back_the_window_content() {
    let mut config = Config {
        workspaces: ["first"]
            .into_iter()
            .map(|name| Workspace {
                name: WorkspaceName(name.into()),
                open_on_output: None,
                layout: None,
            })
            .collect(),
        ..Default::default()
    };
    config.hotkey_overlay.skip_at_startup = true;
    config.layout.auto_stack_new_windows = false;

    let mut f = Fixture::with_config(config);
    f.init_gles_renderer();
    f.add_output(1, (1280, 800));
    let output = f.clarion_output(1);
    f.clarion().output_resized(&output);
    let client = f.add_client();
    // A solid blue window makes the BGRA channel order visible in the result.
    add_window(&mut f, client, [0, 0, u32::MAX, u32::MAX]);
    f.clarion().update_render_elements(None);
    f.double_roundtrip(client);
    f.clarion().update_render_elements(None);
    f.clarion_complete_animations();

    let id = f
        .clarion()
        .layout
        .active_monitor_ref()
        .unwrap()
        .active_workspace_ref()
        .active_window()
        .unwrap()
        .core_id();
    let thumbnails = f.clarion_state().render_window_thumbnails(&[id], 256);
    let thumbnail = thumbnails[0].as_ref().expect("thumbnail");
    assert_eq!(thumbnail.width, 256);
    assert!(thumbnail.height > 0 && thumbnail.height < 256);

    let bgra = base64::engine::general_purpose::STANDARD
        .decode(&thumbnail.bgra)
        .expect("valid base64");
    assert_eq!(
        bgra.len(),
        (thumbnail.width as usize) * (thumbnail.height as usize) * 4
    );
    let center = ((thumbnail.height as usize / 2) * thumbnail.width as usize
        + thumbnail.width as usize / 2)
        * 4;
    assert!(
        bgra[center] > 200 && bgra[center + 1] < 60 && bgra[center + 2] < 60,
        "the blue window survives the GPU readback as BGRA: {:?}",
        &bgra[center..center + 4]
    );
}
