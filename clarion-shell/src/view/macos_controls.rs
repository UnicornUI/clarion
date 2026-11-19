// SPDX-License-Identifier: GPL-3.0-or-later
//! Compact Control Center content. The popup owns its outer glass and shadow.
use super::*;
use crate::app::panels::{CONTROLS_PADDING, CONTROLS_SCALE};

/// One point of the control centre's reference layout, scaled as a unit. The
/// circle diameter is the baseline: the grid, capsules, cards, fonts and
/// gutters all follow the same factor, so the quick-row capsule still spans
/// exactly two circles and their gutter.
fn scaled(value: f32) -> f32 {
    value * CONTROLS_SCALE
}

#[derive(IntoElement)]
struct ControlText {
    text: String,
    size: f32,
    weight: FontWeight,
    color: Hsla,
}

impl RenderOnce for ControlText {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = scaled(self.size);
        div()
            .text_size(px(size))
            .line_height(px(size + 4. * CONTROLS_SCALE))
            .whitespace_nowrap()
            .child(menu_text(self.text, self.weight, self.color, cx))
    }
}

fn clock_time(seconds: f64) -> String {
    let seconds = seconds.max(0.).round() as u64;
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            (seconds % 3600) / 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

fn text(text: impl Into<String>, size: f32, weight: FontWeight, color: Hsla) -> ControlText {
    ControlText {
        text: text.into(),
        size,
        weight,
        color,
    }
}

/// The control center's inner cards step down from the popup radius so the
/// nested surfaces share one silhouette.
const CONTROL_CARD_RADIUS: f32 = 18.;
/// The brightness and volume rows share the inner-card radius.
const SLIDER_CARD_RADIUS: f32 = 18.;
/// The rounded track of both sliders; the prefix glyph is centred inside it.
/// The control center scales it, the battery popup keeps it at rest size.
const SLIDER_TRACK_HEIGHT: f32 = 26.;
/// The prefix glyph of both sliders. It is concentric with the track's rounded
/// left cap, so it also centres under the thumb when the value is 0.
const SLIDER_ICON_SIZE: f32 = 20.;
/// The handle diameter; its radius equals the track's corner radius.
const SLIDER_THUMB: f32 = 24.;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PillKind {
    Network,
    Bluetooth,
    Dnd,
}

pub(super) fn card(p: Palette) -> Div {
    rounded_card(p, 11.)
}

fn rounded_card(p: Palette, radius: f32) -> Div {
    div()
        .rounded(px(radius))
        .border_1()
        .border_color(p.card_line)
        // One uniform fill: a vertical wash made nested cards look like they
        // had a gradient rather than a flat glass tile.
        .bg(p.card)
        .shadow(vec![BoxShadow {
            color: p.shadow.opacity(0.5),
            offset: point(px(0.), px(1.)),
            blur_radius: px(3.),
            spread_radius: px(0.),
        }])
}

/// The control center's own card recipe: the shared fill with the larger
/// radius that matches the popup. The standalone battery popup shares it so
/// the two popups read as one family.
pub(super) fn control_card(p: Palette) -> Div {
    rounded_card(p, CONTROL_CARD_RADIUS)
}

/// The control center's own card, scaled with the rest of the sheet. The
/// battery popup keeps [`control_card`]'s unscaled radius.
fn control_center_card(p: Palette) -> Div {
    rounded_card(p, scaled(CONTROL_CARD_RADIUS))
}

fn percentage(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| format!("{}%", (value.clamp(0., 1.) * 100.).round()))
        .unwrap_or_else(|| "不可用".into())
}

// `slider_value` keeps an eight-point end inset of its own; the extra inset
// here puts the handle's centre on the track's rounded caps at both limits.
fn gesture_bounds(bounds: Bounds<Pixels>, track_height: f32) -> Bounds<Pixels> {
    let inset = (track_height / 2. - 8.).max(0.);
    Bounds::new(
        point(bounds.left() + px(inset), bounds.top()),
        size(
            (bounds.size.width - px(inset * 2.)).max(px(1.)),
            bounds.size.height,
        ),
    )
}

impl ShellView {
    pub(super) fn macos_controls(
        &self,
        host: &ViewState,
        p: Palette,
        popup_width: f32,
    ) -> AnyElement {
        let dark = host.dark();
        // The popup's own padding and rim come off the row width. The rows
        // carry explicit pixels instead of percentages: a percentage inside the
        // popup's scroll wrapper can resolve against an indefinite width on the
        // first layout, which collapsed the sliders to their content for a
        // moment before the next repaint.
        let row_width = (popup_width - 2. * CONTROLS_PADDING - 2.).max(scaled(120.));
        // The quick row pairs one capsule with two circles, so the circles are
        // sized from the row instead of being fixed: four circles and three
        // gaps fill `row_width`, which makes the capsule exactly as wide as one
        // connectivity capsule and lines the four action buttons up with the
        // connectivity split.
        const GAP: f32 = 16. * CONTROLS_SCALE;
        let circle = (row_width - GAP * 3.) / 4.;
        let output = self.output.clone();
        let brightness = host
            .desktop
            .outputs
            .get(&output)
            .filter(|output| output.hdr_active)
            .and_then(|output| output.hdr_brightness)
            .map(|value| f64::from(value) / 100.)
            .or(host.system.brightness);
        let fullscreen = host.prefs.launcher_presentation
            == crate::preferences::LauncherPresentation::Fullscreen;
        let dwindle = matches!(host.tiling_mode, clarion_ipc::TilingMode::Dwindle);
        let dock_autohide = host.prefs.dock_auto_hide;

        let launcher_glyph = self.glyph(
            &self.themed_glyph_path(
                if fullscreen {
                    "move-symbolic"
                } else {
                    "window-new-symbolic"
                },
                if fullscreen {
                    "display.svg"
                } else {
                    "generic-application.svg"
                },
            ),
            scaled(36.),
            Palette {
                fg: if fullscreen {
                    rgb(0xffffff).into()
                } else {
                    p.fg
                },
                ..p
            },
        );
        let layout_glyph = self.glyph(
            &self.themed_glyph_path(
                if dwindle {
                    "view-grid-symbolic"
                } else {
                    "multitasking-symbolic"
                },
                if dwindle { "grid.svg" } else { "display.svg" },
            ),
            scaled(36.),
            Palette {
                fg: if dwindle {
                    rgb(0xffffff).into()
                } else {
                    p.fg
                },
                ..p
            },
        );
        let settings_glyph = self.glyph(
            &self.themed_glyph_path("preferences-system-symbolic", "settings.svg"),
            36.,
            p,
        );
        let shot_glyph = self.glyph(
            &self.themed_glyph_path("screenshooter-symbolic", "scan.svg"),
            36.,
            p,
        );
        let dock_glyph = self.glyph(
            &self.themed_glyph_path("display-symbolic", "display.svg"),
            36.,
            Palette {
                fg: if dock_autohide {
                    rgb(0xffffff).into()
                } else {
                    p.fg
                },
                ..p
            },
        );

        // The media banner spans the full row; the two connectivity capsules
        // and the quick toggles follow underneath, then the action row sits
        // above the brightness and sound sliders.
        let connectivity = div()
            .w(px(row_width))
            .flex()
            .gap(px(GAP))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(self.status_pill(host, PillKind::Network, dark, p)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(self.status_pill(host, PillKind::Bluetooth, dark, p)),
            );
        let quick = div()
            .w(px(row_width))
            .flex()
            .items_center()
            .gap(px(GAP))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(self.status_pill(host, PillKind::Dnd, dark, p)),
            )
            .child(self.control_circle(
                "controls-theme",
                self.symbol(
                    if dark { "moon" } else { "sun" },
                    scaled(36.),
                    Palette {
                        fg: if dark { rgb(0xffffff).into() } else { p.fg },
                        ..p
                    },
                ),
                circle,
                dark,
                Some(Action::Theme(if dark { "light" } else { "dark" }.into())),
                p,
            ))
            .child(self.control_circle(
                "controls-layout",
                layout_glyph,
                circle,
                dwindle,
                Some(Action::ToggleTilingMode),
                p,
            ));

        // Four transport-style actions, spread across the popup width.
        let actions = div()
            .w(px(row_width))
            .flex()
            .gap(px(GAP))
            .child(self.control_circle(
                "controls-settings",
                settings_glyph,
                circle,
                false,
                Some(Action::Panel(Panel::Preferences)),
                p,
            ))
            .child(self.control_circle(
                "controls-screenshot",
                shot_glyph,
                circle,
                false,
                Some(Action::Panel(Panel::Screenshot)),
                p,
            ))
            .child(self.control_circle(
                "controls-launcher",
                launcher_glyph,
                circle,
                fullscreen,
                Some(Action::LauncherPresentation(if fullscreen {
                    crate::preferences::LauncherPresentation::Palette
                } else {
                    crate::preferences::LauncherPresentation::Fullscreen
                })),
                p,
            ))
            .child(self.control_circle(
                "controls-dock",
                dock_glyph,
                circle,
                dock_autohide,
                Some(Action::AutoHide(!dock_autohide)),
                p,
            ));

        div()
            .w(px(row_width))
            .flex()
            .flex_col()
            .gap(px(GAP))
            .child(self.media_card(host, p, row_width))
            .child(connectivity)
            .child(quick)
            .child(actions)
            .child(self.control_slider(
                "brightness",
                "显示器",
                "sun",
                brightness,
                None,
                move |value| {
                    Action::Backend(backend::Command::SetOutputBrightness {
                        output: output.clone(),
                        value,
                    })
                },
                dark,
                p,
            ))
            .child(self.control_slider(
                "volume",
                "声音",
                if host.system.muted {
                    "speaker-muted"
                } else {
                    "speaker"
                },
                host.system.volume,
                Some((
                    host.system.muted,
                    Action::Backend(backend::Command::SetMuted(!host.system.muted)),
                )),
                |value| Action::Backend(backend::Command::SetVolume(value)),
                dark,
                p,
            ))
            .into_any_element()
    }

    /// One round control-center button. The optional action makes the reserved
    /// slot inert; no circle carries a label, matching the macOS tiles.
    fn control_circle(
        &self,
        id: &'static str,
        glyph: AnyElement,
        diameter: f32,
        selected: bool,
        action: Option<Action>,
        p: Palette,
    ) -> AnyElement {
        let mut circle = div()
            .id(id)
            .debug_selector(move || id.into())
            .size(px(diameter))
            .rounded_full()
            .border_1()
            .border_color(if selected { p.accent } else { p.line })
            .bg(if selected { p.accent } else { p.card })
            .flex()
            .items_center()
            .justify_center()
            .focusable()
            .key_context("ClarionControlButton")
            .focus(move |style| style.border_color(p.accent))
            .child(glyph);
        if let Some(action) = action {
            let host = self.host.clone();
            let keyboard_host = self.host.clone();
            let keyboard_action = action.clone();
            circle = circle
                .cursor_pointer()
                .hover(move |style| {
                    if selected {
                        style
                    } else {
                        style.bg(p.line)
                    }
                })
                .on_click(move |_, _, cx| host.update(cx, |host, cx| host.handle(action.clone(), cx)))
                .on_action({
                    let keyboard_host = keyboard_host.clone();
                    let action = keyboard_action.clone();
                    move |_: &Activate, _, cx| {
                        keyboard_host.update(cx, |host, cx| host.handle(action.clone(), cx));
                        cx.stop_propagation();
                    }
                })
                .on_key_down({
                    let keyboard_host = keyboard_host.clone();
                    let action = keyboard_action.clone();
                    move |event, _, cx| {
                        if event.keystroke.key == "space"
                            && event.keystroke.modifiers == Modifiers::default()
                        {
                            keyboard_host.update(cx, |host, cx| host.handle(action.clone(), cx));
                            cx.stop_propagation();
                        }
                    }
                });
        }
        circle.into_any_element()
    }

    /// The three top-left connectivity capsules. Their half-height radius and
    /// the 56px glyph badge share a centre, so the badge keeps an even inset
    /// from the capsule's rounded end.
    fn status_pill(
        &self,
        host: &ViewState,
        kind: PillKind,
        dark: bool,
        p: Palette,
    ) -> AnyElement {
        let network = &host.system.network;
        let (id, title, subtitle, symbol, selected) = match kind {
            PillKind::Network => (
                "controls-network",
                if network.wireless { "Wi-Fi" } else { "网络" },
                network.network.clone().unwrap_or_else(|| "未连接".into()),
                if network.wireless {
                    "network-wireless-symbolic"
                } else {
                    "network-wired-symbolic"
                },
                false,
            ),
            PillKind::Bluetooth => (
                "controls-bluetooth",
                "蓝牙",
                network
                    .bluetooth
                    .clone()
                    .unwrap_or_else(|| "已关闭".into()),
                if network.bluetooth.is_some() {
                    "bluetooth-active-symbolic"
                } else {
                    "bluetooth-disabled-symbolic"
                },
                false,
            ),
            PillKind::Dnd => (
                "controls-dnd",
                "勿扰",
                if host.dnd { "已开启" } else { "已关闭" }.into(),
                if host.dnd { "bell-off" } else { "bell" },
                host.dnd,
            ),
        };
        let glyph_palette = Palette {
            fg: if selected {
                rgb(0xffffff).into()
            } else {
                p.accent
            },
            ..p
        };
        let glyph = match kind {
            PillKind::Dnd => self.symbol(symbol, scaled(30.), glyph_palette),
            _ => self
                .named_icon(symbol, scaled(30.), glyph_palette)
                .unwrap_or_else(|| self.symbol("circle", scaled(28.), glyph_palette)),
        };
        let action = matches!(kind, PillKind::Dnd).then(|| Action::DoNotDisturb(!host.dnd));
        let mut pill = control_center_card(p)
            .rounded_full()
            .id(id)
            .debug_selector(move || id.into())
            .w_full()
            .h(px(scaled(96.)))
            .p(px(scaled(17.)))
            .flex()
            .items_center()
            .gap(px(scaled(13.)))
            .child(
                div()
                    .size(px(scaled(60.)))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(if selected {
                        p.accent
                    } else if dark {
                        rgba(0xffffff1f).into()
                    } else {
                        rgba(0xffffffe6).into()
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(scaled(2.)))
                    .overflow_hidden()
                    .child(text(title, 14., FontWeight::SEMIBOLD, p.fg))
                    .child(text(subtitle, 12., FontWeight::NORMAL, p.muted)),
            );
        if let Some(action) = action {
            let host = self.host.clone();
            let keyboard_host = self.host.clone();
            let keyboard_action = action.clone();
            pill = pill
                .focusable()
                .key_context("ClarionControlButton")
                .focus(move |style| style.border_color(p.accent))
                .cursor_pointer()
                .hover(move |style| style.bg(p.line))
                .on_click(move |_, _, cx| {
                    host.update(cx, |host, cx| host.handle(action.clone(), cx))
                })
                .on_action({
                    let keyboard_host = keyboard_host.clone();
                    let action = keyboard_action.clone();
                    move |_: &Activate, _, cx| {
                        keyboard_host.update(cx, |host, cx| host.handle(action.clone(), cx));
                        cx.stop_propagation();
                    }
                })
                .on_key_down({
                    let keyboard_host = keyboard_host.clone();
                    let action = keyboard_action.clone();
                    move |event, _, cx| {
                        if event.keystroke.key == "space"
                            && event.keystroke.modifiers == Modifiers::default()
                        {
                            keyboard_host.update(cx, |host, cx| host.handle(action.clone(), cx));
                            cx.stop_propagation();
                        }
                    }
                });
        }
        pill.into_any_element()
    }

    /// The square media tile: artwork, track text and the transport row, all
    /// stacked in one column like the reference Now Playing widget.
    fn media_card(&self, host: &ViewState, p: Palette, row_width: f32) -> AnyElement {
        let media = host.system.media.as_ref();
        let playing = media.is_some_and(|media| media.playing);
        let title = media
            .map(|media| media.title.as_str())
            .filter(|title| !title.is_empty())
            .unwrap_or("未在播放")
            .to_owned();
        let artist = media
            .map(|media| media.artist.clone())
            .unwrap_or_default();
        let enabled = media.is_some();
        let art = media.and_then(|media| media.art_png.as_ref()).and_then(|bytes| {
            use sha2::{Digest, Sha256};
            let hash: [u8; 32] = Sha256::digest(bytes).into();
            self.image_cache
                .get(&format!("media-art:{}", icon_key(&hash)))
                .cloned()
        });
        let mut info = div()
            .flex()
            .flex_col()
            .gap(px(scaled(1.)))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .child(text(title, 15., FontWeight::SEMIBOLD, p.fg)),
            );
        if !artist.is_empty() {
            info = info.child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .child(text(artist, 12., FontWeight::NORMAL, p.muted)),
            );
        }
        let controls = div()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(scaled(12.)))
            .child(self.media_button(
                "controls-media-prev",
                "media-skip-backward-symbolic",
                "⏮",
                backend::MediaControl::Previous,
                enabled,
                p,
            ))
            .child(self.media_button(
                "controls-media-play",
                if playing {
                    "media-playback-pause-symbolic"
                } else {
                    "media-playback-start-symbolic"
                },
                if playing { "⏸" } else { "▶" },
                backend::MediaControl::PlayPause,
                enabled,
                p,
            ))
            .child(self.media_button(
                "controls-media-next",
                "media-skip-forward-symbolic",
                "⏭",
                backend::MediaControl::Next,
                enabled,
                p,
            ));
        let progress = media
            .filter(|media| media.length.is_some_and(|length| length > 0.))
            .map(|media| {
                let length = media.length.unwrap_or(0.);
                let position = media.position.unwrap_or(0.).clamp(0., length);
                let fraction = (position / length).clamp(0., 1.) as f32;
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(scaled(6.)))
                    .child(text(clock_time(position), 10., FontWeight::NORMAL, p.muted))
                    .child(
                        div()
                            .flex_1()
                            .h(px(scaled(4.)))
                            .rounded_full()
                            .bg(p.line.opacity(0.6))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(fraction))
                                    .rounded_full()
                                    .bg(p.accent),
                            ),
                    )
                    .child(text(clock_time(length), 10., FontWeight::NORMAL, p.muted))
                    .into_any_element()
            });
        control_center_card(p)
            .debug_selector(|| "controls-media".into())
            .w(px(row_width))
            .h(px(scaled(140.)))
            .p(px(scaled(16.)))
            .flex()
            .items_center()
            .gap(px(scaled(16.)))
            .child(match art {
                // The tile keeps the card's inner height; its 6px corner radius
                // is the card's 18px radius minus the 12px inset, so both arcs
                // share a centre. The artwork is *contained*, not cropped: a
                // cover with an odd ratio keeps its whole frame and the square
                // only pads it. The matte and the clip box carry the same
                // radius, and the image itself is rounded too, so a tall cover
                // (limited by height, narrower than the tile) still ends in
                // arcs rather than square corners.
                Some(image) => div()
                    .size(px(scaled(106.)))
                    .flex_shrink_0()
                    .rounded(px(scaled(6.)))
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(if host.dark() {
                        rgba(0xffffff12)
                    } else {
                        rgba(0x0000000d)
                    })
                    .child(
                        img(image)
                            .size_full()
                            .rounded(px(scaled(6.)))
                            .object_fit(ObjectFit::Contain),
                    )
                    .into_any_element(),
                None => div()
                    .size(px(scaled(106.)))
                    .flex_shrink_0()
                    .rounded(px(scaled(6.)))
                    .bg(linear_gradient(
                        135.,
                        linear_color_stop(rgb(0x3b82f6), 0.),
                        linear_color_stop(rgb(0x8b5cf6), 1.),
                    ))
                    .opacity(if playing { 1. } else { 0.55 })
                    .into_any_element(),
            })
            .child({
                let mut column = div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap(px(scaled(4.)))
                    .child(info)
                    .child(div().flex_1());
                if let Some(progress) = progress {
                    column = column.child(progress);
                }
                column.child(controls)
            })
            .into_any_element()
    }

    fn media_button(
        &self,
        id: &'static str,
        symbol: &str,
        fallback: &'static str,
        control: backend::MediaControl,
        enabled: bool,
        p: Palette,
    ) -> AnyElement {
        let host = self.host.clone();
        let color = if enabled { p.fg } else { p.muted };
        let mut button = div()
            .id(id)
            .debug_selector(move || id.into())
            .size(px(scaled(42.)))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .child(self.media_glyph(symbol, fallback, 24., Palette { fg: color, ..p }));
        if enabled {
            button = button
                .cursor_pointer()
                .hover(move |style| style.bg(p.line))
                .on_click(move |_, _, cx| {
                    host.update(cx, |host, cx| {
                        host.handle(
                            Action::Backend(backend::Command::Media(control)),
                            cx,
                        )
                    })
                });
        }
        button.into_any_element()
    }

    /// Prefer the theme's transport glyph; text glyphs keep the row readable on
    /// icon sets that ship no media artwork.
    fn media_glyph(&self, symbol: &str, fallback: &'static str, size: f32, p: Palette) -> AnyElement {
        let size = scaled(size);
        let path = crate::icons::resolve(symbol);
        match path {
            Some(path) => self.glyph(&path, size, p),
            None => div()
                .text_size(px(size))
                .line_height(px(size + 2. * CONTROLS_SCALE))
                .text_color(p.fg)
                .child(fallback)
                .into_any_element(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn control_slider(
        &self,
        id: &'static str,
        label: &'static str,
        icon: &'static str,
        value: Option<f64>,
        mute: Option<(bool, Action)>,
        action: impl Fn(f64) -> Action + Clone + 'static,
        dark: bool,
        p: Palette,
    ) -> AnyElement {
        let value = value
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(0., 1.));
        let mut trailing = div().flex().items_center().gap(px(scaled(10.))).child(text(
            percentage(value),
            13.,
            FontWeight::NORMAL,
            p.muted,
        ));
        if let Some((muted, mute_action)) = mute.filter(|_| value.is_some()) {
            let host = self.host.clone();
            let keyboard_host = self.host.clone();
            let keyboard_action = mute_action.clone();
            trailing = trailing.child(
                div()
                    .id("controls-mute")
                    .debug_selector(|| "controls-mute".into())
                    .h(px(scaled(26.)))
                    .px(px(scaled(7.)))
                    .rounded(px(scaled(5.)))
                    .focusable()
                    .key_context("ClarionControlButton")
                    .cursor_pointer()
                    .hover(move |style| style.bg(p.line))
                    .focus(move |style| style.bg(p.line))
                    .child(text(
                        if muted { "取消静音" } else { "静音" },
                        13.,
                        FontWeight::NORMAL,
                        p.fg,
                    ))
                    .on_click(move |_, _, cx| {
                        host.update(cx, |host, cx| host.handle(mute_action.clone(), cx))
                    })
                    .on_action({
                        let host = keyboard_host.clone();
                        let action = keyboard_action.clone();
                        move |_: &Activate, _, cx| {
                            host.update(cx, |host, cx| host.handle(action.clone(), cx));
                            cx.stop_propagation();
                        }
                    })
                    .on_key_down(move |event, _, cx| {
                        if event.keystroke.key == "space"
                            && event.keystroke.modifiers == Modifiers::default()
                        {
                            keyboard_host
                                .update(cx, |host, cx| host.handle(keyboard_action.clone(), cx));
                            cx.stop_propagation();
                        }
                    }),
            );
        }
        let body = control_center_card(p)
            .debug_selector(move || format!("controls-slider-{id}"))
            .rounded(px(scaled(SLIDER_CARD_RADIUS)))
            .w_full()
            .px(px(scaled(16.)))
            .py(px(scaled(14.)))
            .flex()
            .flex_col()
            .gap(px(scaled(10.)))
            .child(
                div()
                    .h(px(scaled(24.)))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(text(label, 15., FontWeight::SEMIBOLD, p.fg))
                    .child(trailing),
            );
        let Some(value) = value else {
            return body
                .child(
                    div()
                        .h(px(scaled(SLIDER_TRACK_HEIGHT)))
                        .w_full()
                        .rounded_full()
                        .bg(if dark {
                            rgba(0x00000022)
                        } else {
                            rgba(0x00000010)
                        })
                        .pl(px((scaled(SLIDER_TRACK_HEIGHT) - scaled(SLIDER_ICON_SIZE)) / 2.))
                        .flex()
                        .items_center()
                        .child(self.symbol(
                            icon,
                            scaled(SLIDER_ICON_SIZE),
                            Palette { fg: p.muted, ..p },
                        )),
                )
                .into_any_element();
        };
        body.child(ControlSlider {
            key: SharedString::from(format!("control-slider-{id}")),
            selector: format!("slider-{id}"),
            host: self.host.clone(),
            action: std::rc::Rc::new(action),
            value,
            dark,
            p,
            scale: CONTROLS_SCALE,
            icon: self.symbol(
                icon,
                scaled(SLIDER_ICON_SIZE),
                Palette {
                    fg: rgb(0x707077).into(),
                    ..p
                },
            ),
        })
        .into_any_element()
    }
}

#[derive(IntoElement)]
pub(super) struct ControlSlider {
    pub(super) key: SharedString,
    pub(super) selector: String,
    pub(super) host: Entity<Host>,
    pub(super) action: std::rc::Rc<dyn Fn(f64) -> Action>,
    pub(super) value: f64,
    pub(super) dark: bool,
    pub(super) p: Palette,
    /// The control center scales its sliders down with the rest of the sheet;
    /// the battery popup keeps the reference size.
    pub(super) scale: f32,
    pub(super) icon: AnyElement,
}

#[allow(clippy::too_many_arguments)]
fn step_slider(
    gesture: &Entity<SliderGesture>,
    value: f64,
    step: f64,
    host: &Entity<Host>,
    action: &std::rc::Rc<dyn Fn(f64) -> Action>,
    window: &mut Window,
    cx: &mut App,
) {
    let next = gesture.update(cx, |state, cx| {
        let current = state.preview.map(|(value, _)| value).unwrap_or(value);
        let next = (current + step).clamp(0., 1.);
        let now = std::time::Instant::now();
        state.preview = Some((next, now));
        state.last_sent = Some(now);
        state.last_sent_value = Some(next);
        cx.notify();
        next
    });
    host.update(cx, |host, cx| host.handle(action(next), cx));
    window.refresh();
}

impl RenderOnce for ControlSlider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            key,
            selector,
            host,
            action,
            value,
            dark,
            p,
            scale,
            icon,
        } = self;
        let track = SLIDER_TRACK_HEIGHT * scale;
        let thumb_size = SLIDER_THUMB * scale;
        // RenderOnce runs in GPUI's prepaint/layout phase; state cannot be
        // created later from a keyboard callback or on a different canvas node.
        let gesture = window.use_keyed_state(key.clone(), cx, |_, _| SliderGesture::default());
        let focus_state =
            window.use_keyed_state(SharedString::from(format!("{key}-focus")), cx, |_, cx| {
                cx.focus_handle()
            });
        let focus = focus_state.read(cx).clone();
        let icon_selector = format!("{selector}-icon");
        div()
            .id(key.clone())
            .relative()
            .h(px(track))
            .flex_shrink_0()
            .w_full()
            .track_focus(&focus)
            .rounded_full()
            .focus(move |style| {
                style.shadow(vec![BoxShadow {
                    color: p.accent.opacity(0.55),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(2. * scale),
                    spread_radius: px(2. * scale),
                }])
            })
            .key_context("ClarionControlSlider")
            .on_action({
                let gesture = gesture.clone();
                let host = host.clone();
                let action = action.clone();
                move |_: &Previous, window, cx| {
                    step_slider(&gesture, value, 0.05, &host, &action, window, cx);
                    cx.stop_propagation();
                }
            })
            .on_action({
                let gesture = gesture.clone();
                let host = host.clone();
                let action = action.clone();
                move |_: &Next, window, cx| {
                    step_slider(&gesture, value, -0.05, &host, &action, window, cx);
                    cx.stop_propagation();
                }
            })
            .on_key_down({
                let gesture = gesture.clone();
                let host = host.clone();
                let action = action.clone();
                move |event, window, cx| {
                    if event.keystroke.modifiers != Modifiers::default() {
                        return;
                    }
                    let step = match event.keystroke.key.as_str() {
                        "left" => -0.05,
                        "right" => 0.05,
                        _ => return,
                    };
                    step_slider(&gesture, value, step, &host, &action, window, cx);
                    cx.stop_propagation();
                }
            })
            .cursor_pointer()
            .debug_selector(move || selector)
            .child(
                canvas(
                    move |bounds, window, _| {
                        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                        (gesture, hitbox)
                    },
                    move |bounds, (gesture, hitbox), window, cx| {
                        let displayed = gesture.update(cx, |state, _| {
                            if !state.dragging
                                && state.preview.is_some_and(|(preview, changed)| {
                                    (preview - value).abs() < 0.001
                                        || changed.elapsed() > std::time::Duration::from_secs(2)
                                })
                            {
                                state.preview = None;
                            }
                            state
                                .preview
                                .map(|(value, _)| value)
                                .unwrap_or(value)
                                .clamp(0., 1.)
                        });
                        let interaction = gesture_bounds(bounds, track);
                        let start = interaction.left() + px(8.);
                        let width = (interaction.size.width - px(16.)).max(px(1.));
                        let thumb_x = start + width * displayed as f32;
                        window.paint_quad(
                            fill(
                                bounds,
                                if dark {
                                    rgba(0x00000040)
                                } else {
                                    rgba(0x58586229)
                                },
                            )
                            .corner_radii(px(track / 2.)),
                        );
                        window.paint_quad(
                            fill(
                                Bounds::new(
                                    bounds.origin,
                                    size(
                                        (thumb_x - bounds.left() + px(track / 2.)).min(bounds.size.width),
                                        bounds.size.height,
                                    ),
                                ),
                                linear_gradient(
                                    180.,
                                    linear_color_stop(rgba(0xfffffff5), 0.),
                                    linear_color_stop(rgba(0xffffffe0), 1.),
                                ),
                            )
                            .corner_radii(px(track / 2.)),
                        );
                        let thumb = Bounds::new(
                            point(
                                thumb_x - px(thumb_size / 2.),
                                bounds.top() + px((track - thumb_size) / 2.),
                            ),
                            size(px(thumb_size), px(thumb_size)),
                        );
                        window.paint_shadows(
                            thumb,
                            Corners::all(px(thumb_size / 2.)),
                            &[BoxShadow {
                                color: rgba(0x00000024).into(),
                                offset: point(px(0.), px(scale)),
                                blur_radius: px(3. * scale),
                                spread_radius: px(0.),
                            }],
                        );
                        window.paint_quad(
                            fill(thumb, rgb(0xffffff))
                                .corner_radii(px(thumb_size / 2.))
                                .border_widths(px(0.5))
                                .border_color(rgba(0x00000012)),
                        );
                        // The focused outer Div owns keyboard dispatch. The canvas
                        // only draws the track and manages pointer capture.
                        let down_state = gesture.clone();
                        let down_host = host.clone();
                        let down_action = action.clone();
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble
                                || event.button != MouseButton::Left
                                || !hitbox.is_hovered(window)
                            {
                                return;
                            }
                            window.focus(&focus);
                            window.prevent_default();
                            down_state.update(cx, |state, _| state.dragging = true);
                            if let Some(value) = update_slider(
                                &down_state,
                                interaction,
                                event.position.x,
                                true,
                                window,
                                cx,
                            ) {
                                down_host
                                    .update(cx, |host, cx| host.handle(down_action(value), cx));
                            }
                            cx.stop_propagation();
                        });
                        let move_state = gesture.clone();
                        let move_host = host.clone();
                        let move_action = action.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble || !move_state.read(cx).dragging {
                                return;
                            }
                            if event.pressed_button != Some(MouseButton::Left) {
                                move_state.update(cx, |state, cx| {
                                    state.dragging = false;
                                    cx.notify();
                                });
                                return;
                            }
                            if let Some(value) = update_slider(
                                &move_state,
                                interaction,
                                event.position.x,
                                false,
                                window,
                                cx,
                            ) {
                                move_host
                                    .update(cx, |host, cx| host.handle(move_action(value), cx));
                            }
                        });
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble
                                || event.button != MouseButton::Left
                                || !gesture.read(cx).dragging
                            {
                                return;
                            }
                            if let Some(value) = update_slider(
                                &gesture,
                                interaction,
                                event.position.x,
                                true,
                                window,
                                cx,
                            ) {
                                host.update(cx, |host, cx| host.handle(action(value), cx));
                            }
                            gesture.update(cx, |state, cx| {
                                state.dragging = false;
                                cx.notify();
                            });
                            cx.stop_propagation();
                        });
                    },
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(0.))
                    .size(px(track))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().debug_selector(move || icon_selector).child(icon)),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ::core::prelude::v1::test;
    use clarion_launcher::catalog::AppCatalog;

    use super::*;
    use crate::app::testing;

    fn private_host(cx: &mut TestAppContext) -> (tempfile::TempDir, Entity<Host>) {
        let root = tempfile::tempdir().unwrap();
        let private = root.path().join("private").to_string_lossy().into_owned();
        let environment: HashMap<String, String> = [
            ("HOME".into(), root.path().to_string_lossy().into_owned()),
            ("XDG_CONFIG_HOME".into(), private.clone()),
            ("XDG_CONFIG_DIRS".into(), private.clone()),
            ("XDG_DATA_HOME".into(), private.clone()),
            ("XDG_DATA_DIRS".into(), private),
        ]
        .into();
        let catalog = AppCatalog::load(&environment).unwrap();
        cx.update(bind_keys);
        let host = cx.update(|cx| testing::test_host(cx, catalog, environment));
        (root, host)
    }

    fn activate(visual: &mut VisualTestContext, width: f32, height: f32) {
        visual.update(|window, _| {
            window.resize(size(px(width), px(height)));
            window.activate_window();
            window.refresh();
        });
        visual.run_until_parked();
    }

    // Focus through the real pointer path, cancelling only the subsequent click.
    fn focus_without_click(visual: &mut VisualTestContext, selector: &'static str) {
        let bounds = visual
            .debug_bounds(selector)
            .expect("actual control bounds");
        visual.simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_up(
            point(px(-20.), px(-20.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.run_until_parked();
    }

    #[gpui::test]
    fn the_control_center_rows_share_one_width_and_centre_their_transport(
        cx: &mut TestAppContext,
    ) {
        let (_root, host) = private_host(cx);
        host.update(cx, |host, cx| {
            host.system.brightness = Some(0.4);
            host.system.volume = Some(0.62);
            host.changed(cx);
        });
        let window = cx.update(|cx| testing::open_panel(&host, Panel::Controls, cx));
        let mut visual = VisualTestContext::from_window(*window, cx);
        activate(&mut visual, 1920., 1080.);
        let media = visual
            .debug_bounds("controls-media")
            .expect("the media card is painted");
        // 399 popup - the scaled body padding a side - the 1px rim a side.
        assert_eq!(f32::from(media.size.width), 369.);
        for id in [
            "controls-network",
            "controls-dnd",
            "controls-theme",
            "controls-settings",
        ] {
            let bounds = visual.debug_bounds(id).expect("row child is painted");
            assert!(
                f32::from(bounds.origin.x) >= f32::from(media.origin.x),
                "{id} starts inside the card column"
            );
        }
        // The quick-row capsule is as wide as a connectivity capsule, and the
        // four action circles sit in the same columns with 16px gutters.
        let network = visual
            .debug_bounds("controls-network")
            .expect("the network capsule is painted");
        let bluetooth = visual
            .debug_bounds("controls-bluetooth")
            .expect("the bluetooth capsule is painted");
        let dnd = visual
            .debug_bounds("controls-dnd")
            .expect("the do-not-disturb capsule is painted");
        assert!(
            (f32::from(network.size.width) - (369. - scaled(16.)) / 2.).abs() <= 0.5,
            "the connectivity capsules split the row around one gutter ({})",
            f32::from(network.size.width)
        );
        assert_eq!(
            f32::from(bluetooth.size.width),
            f32::from(network.size.width)
        );
        assert_eq!(
            f32::from(dnd.size.width),
            f32::from(network.size.width),
            "the quick-row capsule spans two capsules and their gutter"
        );
        let mut right: Option<f32> = None;
        for id in [
            "controls-settings",
            "controls-screenshot",
            "controls-launcher",
            "controls-dock",
        ] {
            let bounds = visual.debug_bounds(id).expect("action circle is painted");
            if let Some(right) = right {
                assert!(
                    (f32::from(bounds.origin.x) - right - scaled(16.)).abs() <= 1.,
                    "{id} keeps a scaled gutter"
                );
            }
            right = Some(f32::from(bounds.origin.x) + f32::from(bounds.size.width));
        }
        assert!(right.is_some());
        // The rows keep the same 16px gutter vertically: media, connectivity,
        // quick toggles, actions and the two slider cards.
        let settings = visual
            .debug_bounds("controls-settings")
            .expect("the first action circle is painted");
        let brightness_card = visual
            .debug_bounds("controls-slider-brightness")
            .expect("the brightness card is painted");
        for (label, lower, upper) in [
            ("media/connectivity", media, network),
            ("connectivity/quick", network, dnd),
            ("quick/actions", dnd, settings),
            ("actions/brightness", settings, brightness_card),
        ] {
            let gap = f32::from(upper.origin.y)
                - (f32::from(lower.origin.y) + f32::from(lower.size.height));
            assert!(
                (gap - scaled(16.)).abs() <= 1.,
                "{label} keeps a scaled gutter ({gap})"
            );
        }
        let brightness = visual
            .debug_bounds("slider-brightness")
            .expect("the brightness slider is painted");
        assert!(
            (f32::from(brightness.size.width) - (369. - scaled(16.) * 2. - 2.)).abs() <= 0.5,
            "the slider card keeps the full row width instead of collapsing to its content ({})",
            f32::from(brightness.size.width)
        );
        assert!(
            (f32::from(brightness.size.height) - scaled(SLIDER_TRACK_HEIGHT)).abs() <= 0.5,
            "the track keeps its full height inside the shorter card ({})",
            f32::from(brightness.size.height)
        );
        assert!(
            brightness.origin.y >= brightness_card.origin.y
                && brightness.bottom() <= brightness_card.bottom(),
            "the track stays inside the card"
        );
        let prefix = visual
            .debug_bounds("slider-brightness-icon")
            .expect("the prefix glyph is painted");
        let track_center = f32::from(brightness.origin.y) + f32::from(brightness.size.height) / 2.;
        let prefix_center = f32::from(prefix.origin.y) + f32::from(prefix.size.height) / 2.;
        assert!(
            (track_center - prefix_center).abs() <= 0.5,
            "the prefix glyph is centred in the track ({prefix_center} vs {track_center})"
        );
        assert!(prefix.origin.y >= brightness.origin.y && prefix.bottom() <= brightness.bottom());
        let cap_center = f32::from(brightness.origin.x) + scaled(SLIDER_TRACK_HEIGHT) / 2.;
        let prefix_center_x = f32::from(prefix.origin.x) + f32::from(prefix.size.width) / 2.;
        assert!(
            (prefix_center_x - cap_center).abs() <= 0.5,
            "the prefix glyph is concentric with the left cap ({prefix_center_x} vs {cap_center})"
        );
        let play = visual
            .debug_bounds("controls-media-play")
            .expect("the play button is painted");
        let prev = visual
            .debug_bounds("controls-media-prev")
            .expect("the previous button is painted");
        let next = visual
            .debug_bounds("controls-media-next")
            .expect("the next button is painted");
        // The transport is centred in the column beside the artwork: the play
        // button sits midway between its neighbours and the group keeps equal
        // margins inside the card.
        assert_eq!(f32::from(prev.origin.y), f32::from(next.origin.y));
        let span_center =
            (f32::from(prev.origin.x) + f32::from(next.origin.x) + scaled(42.)) / 2.;
        assert!(
            (f32::from(play.origin.x) + scaled(21.) - span_center).abs() <= 1.,
            "the play button sits in the middle of the transport"
        );
        let left_gap = f32::from(prev.origin.x)
            - (f32::from(media.origin.x) + scaled(16.) + scaled(106.) + scaled(16.));
        let right_gap = (f32::from(media.origin.x) + 369. - scaled(16.))
            - (f32::from(next.origin.x) + scaled(42.));
        assert!(
            (left_gap - right_gap).abs() <= 1.,
            "the transport is centred under the timeline ({left_gap} vs {right_gap})"
        );
    }

    #[gpui::test]
    fn controls_leave_the_battery_details_to_the_standalone_popup(cx: &mut TestAppContext) {
        let (_root, host) = private_host(cx);
        host.update(cx, |host, cx| {
            host.system.battery = Some(0.07);
            host.system.battery_status = crate::backend::BatteryStatus::Discharging;
            host.system.battery_health = Some(0.4968);
            host.system.ac_online = false;
            host.changed(cx);
        });
        let window = cx.update(|cx| testing::open_panel(&host, Panel::Controls, cx));
        let mut visual = VisualTestContext::from_window(*window, cx);
        activate(&mut visual, 360., 510.);
        assert!(
            visual.debug_bounds("controls-battery").is_none(),
            "the control center no longer carries a battery card"
        );
        assert!(
            visual.debug_bounds("battery-summary").is_none(),
            "the battery summary belongs to its own popup"
        );
    }

    #[gpui::test]
    fn controls_switch_appearance_and_launcher_presentation(cx: &mut TestAppContext) {
        let (_root, host) = private_host(cx);
        let window = cx.update(|cx| testing::open_panel(&host, Panel::Controls, cx));
        let mut visual = VisualTestContext::from_window(*window, cx);
        activate(&mut visual, 360., 590.);
        for id in ["controls-theme", "controls-launcher"] {
            assert!(
                visual.debug_bounds(id).is_some(),
                "the control center paints the {id} card"
            );
        }

        // Drive the launcher first so the appearance re-layout cannot move the
        // target between the bounds read and the press.
        let launcher = visual
            .debug_bounds("controls-launcher")
            .expect("launcher circle")
            .center();
        visual.simulate_click(launcher, Modifiers::default());
        visual.run_until_parked();
        host.read_with(cx, |host, _| {
            assert_eq!(
                host.prefs.launcher_presentation,
                crate::preferences::LauncherPresentation::Palette
            )
        });

        let launcher = visual
            .debug_bounds("controls-launcher")
            .expect("launcher circle")
            .center();
        visual.simulate_click(launcher, Modifiers::default());
        visual.run_until_parked();
        host.read_with(cx, |host, _| {
            assert_eq!(
                host.prefs.launcher_presentation,
                crate::preferences::LauncherPresentation::Fullscreen
            )
        });

        let theme = visual
            .debug_bounds("controls-theme")
            .expect("appearance circle")
            .center();
        visual.simulate_click(theme, Modifiers::default());
        visual.run_until_parked();
        host.read_with(cx, |host, _| assert_eq!(host.prefs.theme, "light"));
    }

    #[gpui::test]
    fn controls_tile_enter_and_space_update_real_dnd_after_pointer_focus(cx: &mut TestAppContext) {
        let (_root, host) = private_host(cx);
        let window = cx.update(|cx| testing::open_panel(&host, Panel::Controls, cx));
        let mut visual = VisualTestContext::from_window(*window, cx);
        activate(&mut visual, 360., 424.);
        focus_without_click(&mut visual, "controls-dnd");
        host.read_with(cx, |host, _| assert!(!host.dnd));
        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        host.read_with(cx, |host, _| {
            assert!(host.dnd, "Enter must reach the tile's Activate handler");
            assert_eq!(host.panel, Some(Panel::Controls));
        });
        visual.simulate_keystrokes("space");
        visual.run_until_parked();
        host.read_with(cx, |host, _| {
            assert!(!host.dnd, "Space must toggle the same focused tile")
        });
    }

    #[gpui::test]
    fn controls_enter_opens_the_screenshot_target(cx: &mut TestAppContext) {
        let (_root, host) = private_host(cx);
        let window = cx.update(|cx| testing::open_panel(&host, Panel::Controls, cx));
        let mut visual = VisualTestContext::from_window(*window, cx);
        // The control center carries the top control row, the action row and
        // two sliders.
        activate(&mut visual, 360., 560.);
        focus_without_click(&mut visual, "controls-screenshot");
        host.read_with(cx, |host, _| assert_eq!(host.panel, Some(Panel::Controls)));
        visual.simulate_keystrokes("enter");
        cx.run_until_parked();
        host.read_with(cx, |host, _| {
            assert_eq!(host.panel.as_ref(), Some(&Panel::Screenshot))
        });
        assert!(
            !cx.read(|cx| cx
                .windows()
                .iter()
                .any(|handle| handle.window_id() == window.window_id())),
            "Enter must replace the real popup, not merely change a selection"
        );
    }

    #[gpui::test]
    fn controls_layout_button_toggles_the_workspace_and_the_pills_report_connectivity(
        cx: &mut TestAppContext,
    ) {
        let (_root, host) = private_host(cx);
        host.update(cx, |host, cx| {
            host.system.network = Box::new(crate::backend::NetworkSnapshot {
                network: Some("Home 5G".into()),
                wireless: true,
                bluetooth: Some("Keychron K8".into()),
            });
            host.changed(cx);
        });
        let window = cx.update(|cx| testing::open_panel(&host, Panel::Controls, cx));
        let mut visual = VisualTestContext::from_window(*window, cx);
        activate(&mut visual, 360., 560.);
        assert!(visual.debug_bounds("controls-network").is_some());
        assert!(visual.debug_bounds("controls-bluetooth").is_some());
        assert!(visual.debug_bounds("controls-media").is_some());

        let commands = cx.update(|cx| testing::capture_backend(&host, cx));
        let layout = visual
            .debug_bounds("controls-layout")
            .expect("layout circle");
        visual.simulate_click(layout.center(), Modifiers::default());
        visual.run_until_parked();
        host.read_with(cx, |host, _| {
            assert_eq!(host.tiling_mode, clarion_ipc::TilingMode::Dwindle)
        });
        assert!(matches!(
            commands.try_recv(),
            Ok(backend::Command::Action(
                clarion_ipc::Action::SwitchTilingMode {
                    mode: Some(clarion_ipc::TilingMode::Dwindle)
                }
            ))
        ));
    }

    struct SliderHarness {
        view: Entity<ShellView>,
        host: Entity<Host>,
        focus: FocusHandle,
    }

    impl Render for SliderHarness {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let shell = self.view.read(cx);
            let value = (self.host.read(cx).prefs.bar_height - 22.) / 26.;
            div()
                .w(px(296.))
                .h(px(110.))
                .key_context("ClarionPanel")
                .track_focus(&self.focus)
                .on_action(|_: &Previous, _, _| panic!("Up escaped focused slider"))
                .on_action(|_: &Next, _, _| panic!("Down escaped focused slider"))
                .child(shell.control_slider(
                    "keyboard-regression",
                    "测试亮度",
                    "sun",
                    Some(value),
                    None,
                    |value| Action::BarHeight(22. + 26. * value),
                    true,
                    Palette::new(&shell.state),
                ))
        }
    }

    #[gpui::test]
    fn controls_slider_pointer_focus_and_arrows_change_host_values_across_redraws(
        cx: &mut TestAppContext,
    ) {
        let (_root, host) = private_host(cx);
        let controls = cx.update(|cx| testing::open_panel(&host, Panel::Controls, cx));
        let view = controls.root(cx).unwrap();
        // The production slider receives a real Host action with a numeric
        // result, without enabling a host audio/backlight tool in this test.
        let (harness, visual) = cx.add_window_view(|window, cx| {
            let focus = cx.focus_handle();
            window.focus(&focus);
            cx.observe(&host, |_, _, cx| cx.notify()).detach();
            SliderHarness {
                view,
                host: host.clone(),
                focus,
            }
        });
        activate(visual, 320., 160.);
        let bounds = visual
            .debug_bounds("slider-keyboard-regression")
            .expect("actual capsule track");
        visual.simulate_click(bounds.center(), Modifiers::default());
        visual.run_until_parked();
        host.read_with(visual, |host, _| {
            assert!((host.prefs.bar_height - 35.).abs() < 0.001)
        });
        let focused = visual.update(|window, cx| {
            window
                .focused(cx)
                .expect("track click must focus the slider")
        });
        let root_focus = harness.read_with(visual, |harness, _| harness.focus.clone());
        assert_ne!(
            focused, root_focus,
            "canvas stop_propagation must not leave focus on the ancestor"
        );
        for (key, expected) in [("right", 36.3), ("up", 37.6), ("left", 36.3), ("down", 35.)] {
            visual.simulate_keystrokes(key);
            visual.run_until_parked();
            host.read_with(visual, |host, _| {
                assert!(
                    (host.prefs.bar_height - expected).abs() < 0.001,
                    "{key} did not reach the focused capsule"
                )
            });
            assert_eq!(
                visual.update(|window, cx| window.focused(cx)),
                Some(focused.clone()),
                "slider focus must survive the action's Host redraw"
            );
        }
        visual.simulate_mouse_move(
            point(bounds.right() + px(20.), bounds.center().y),
            None,
            Modifiers::default(),
        );
        host.read_with(visual, |host, _| {
            assert!(
                (host.prefs.bar_height - 35.).abs() < 0.001,
                "keyboard steps must not leave a pointer drag active"
            )
        });
    }

    #[::core::prelude::v1::test]
    fn capsule_gesture_matches_handle_endpoints() {
        let bounds = Bounds::new(point(px(10.), px(20.)), size(px(260.), px(26.)));
        let interaction = gesture_bounds(bounds, SLIDER_TRACK_HEIGHT);
        // The handle's centre reaches each rounded cap exactly: the gesture's
        // inset and the shared slider's eight-point inset compose to the
        // track's half-height at both ends.
        let cap = SLIDER_TRACK_HEIGHT / 2.;
        assert!(slider_value(interaction, bounds.left() + px(cap)).abs() < 1e-6);
        assert!((slider_value(interaction, bounds.right() - px(cap)) - 1.).abs() < 1e-6);
        assert!((slider_value(interaction, px(140.)) - 0.5).abs() < 1e-6);
        assert_eq!(slider_value(interaction, px(-100.)), 0.);
        assert_eq!(slider_value(interaction, px(500.)), 1.);
    }

    #[::core::prelude::v1::test]
    fn unavailable_and_invalid_values_are_not_fake_live_percentages() {
        assert_eq!(percentage(None), "不可用");
        assert_eq!(percentage(Some(f64::NAN)), "不可用");
        assert_eq!(percentage(Some(0.537)), "54%");
        assert_eq!(percentage(Some(-1.)), "0%");
        assert_eq!(percentage(Some(2.)), "100%");
    }
}
