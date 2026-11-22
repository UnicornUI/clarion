//! Native GPUI system settings for Clarion.

mod app;
mod bridge;
mod compositor;
mod icons;
mod input;
mod instance;
mod nav;
mod system_info;
mod theme;
mod widgets;

use clap::Parser;
use std::borrow::Cow;

use gpui::{
    point, px, size, App, AppContext, Application, AssetSource, Bounds, SharedString,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
};

use app::SettingsApp;

#[derive(Parser)]
#[command(version, about = "Clarion system settings")]
struct Cli {
    /// Open one pane, for example `dock`, `appearance` or `displays`.
    #[arg(long)]
    pane: Option<String>,
    /// Start in the light appearance regardless of the desktop preference.
    #[arg(long)]
    light: bool,
    #[arg(long)]
    dark: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.light && cli.dark {
        anyhow::bail!("--light and --dark conflict");
    }
    let initial = cli.pane.as_deref().and_then(nav::from_arg);
    let path = instance::socket_path();
    if instance::signal_existing(&path, cli.pane.as_deref()).unwrap_or(false) {
        return Ok(());
    }
    let (tx, signals) = std::sync::mpsc::channel();
    let instance = match instance::Instance::bind(path.clone(), tx) {
        Ok(instance) => instance,
        Err(error)
            if error.kind() == std::io::ErrorKind::AddrInUse
                && instance::signal_existing(&path, cli.pane.as_deref()).unwrap_or(false) =>
        {
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    // Keep the listener (and its lock) alive for the life of the process.
    std::mem::forget(instance);
    Application::new()
        .with_assets(SettingsAssets)
        .run(move |cx: &mut App| {
            input::TextInput::bind_keys(cx);
            let bounds = Bounds::new(point(px(120.), px(80.)), size(px(1200.), px(860.)));
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                kind: WindowKind::Normal,
                is_movable: true,
                is_resizable: true,
                is_minimizable: true,
                focus: true,
                show: true,
                window_background: WindowBackgroundAppearance::Blurred,
                app_id: Some("clarion-settings".into()),
                window_min_size: Some(size(px(900.), px(680.))),
                window_decorations: Some(gpui::WindowDecorations::Client),
                ..Default::default()
            };
            if let Err(error) = cx.open_window(options, move |window, cx| {
                window.set_window_title("系统设置");
                window.set_client_inset(px(0.));
                let view = cx.new(SettingsApp::new);
                view.update(cx, |view, cx| {
                    view.watch_signals(window, signals, cx);
                    if let Some(pane) = initial {
                        view.select(pane, cx);
                    }
                });
                window.activate_window();
                view
            }) {
                eprintln!("无法打开系统设置窗口：{error}");
            }
        });
    Ok(())
}


struct SettingsAssets;

impl AssetSource for SettingsAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "search.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/search.svg"))),
            _ => None,
        })
    }

    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}
