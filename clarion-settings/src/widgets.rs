//! Reusable GPUI controls for the settings sheet. Pages compose these instead
//! of repeating layout/colors, which keeps the visual rhythm and the behaviour
//! of toggles, segments and sliders in one place.

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    div, fill, img, linear_color_stop, linear_gradient, point, px, rgb, rgba, size, AnyElement,
    Bounds, Image, ImageSource, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ObjectFit, PathBuilder, SharedString, Window,
};

use crate::theme::{self, Palette, CARD_RADIUS, ROW_HEIGHT};

/// Right-hand value column of a slider row. Wide enough for CJK unit labels
/// such as `50 次/秒` or `1.20 ×` without wrapping; the slider itself keeps the
/// same width so the row still aligns with dropdown rows.
const SLIDER_VALUE_WIDTH: f32 = 60.;

pub fn label(
    value: impl Into<String>,
    size: f32,
    weight: gpui::FontWeight,
    color: gpui::Hsla,
    cx: &gpui::App,
) -> impl IntoElement {
    let size = size * theme::TEXT_SCALE;
    div()
        .text_size(px(size))
        .line_height(px(size + 4.))
        .child(theme::text(value, weight, color, cx))
}

pub fn section(
    title: &'static str,
    p: Palette,
    cx: &gpui::App,
    body: impl IntoElement,
) -> impl IntoElement {
    section_with_help(title, None, p, cx, body)
}

/// Section header with an optional superscript help glyph next to the title.
pub fn section_with_help(
    title: &'static str,
    help: Option<AnyElement>,
    p: Palette,
    cx: &gpui::App,
    body: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .mb(px(24.))
        .child(
            div()
                .pl(px(16.))
                .flex()
                .items_center()
                .gap(px(5.))
                .child(label(title, 13., gpui::FontWeight::SEMIBOLD, p.muted, cx))
                .when_some(help, |row, help| row.child(help)),
        )
        .child(card(body, p))
}

/// Section with a small right-aligned hint in the heading row.
pub fn section_hint(
    title: &'static str,
    hint: &'static str,
    p: Palette,
    cx: &gpui::App,
    body: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .mb(px(24.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .pl(px(16.))
                .pr(px(6.))
                .child(label(title, 13., gpui::FontWeight::SEMIBOLD, p.muted, cx))
                .child(label(
                    hint,
                    10.5,
                    gpui::FontWeight::NORMAL,
                    p.muted,
                    cx,
                )),
        )
        .child(card(body, p))
}

/// Card container without a section label, for a single merged row.
pub fn card(body: impl IntoElement, p: Palette) -> impl IntoElement {
    div()
        .rounded(px(CARD_RADIUS))
        .border_1()
        .border_color(p.card_border)
        .bg(p.card)
        .shadow(vec![gpui::BoxShadow {
            color: p.shadow.opacity(0.14),
            offset: point(px(0.), px(1.)),
            blur_radius: px(14.),
            spread_radius: px(0.),
        }])
        .px(px(6.))
        .child(body)
}

pub fn row(
    value: impl Into<String>,
    control: impl IntoElement,
    p: Palette,
    cx: &gpui::App,
) -> impl IntoElement {
    row_with_help(value, None, control, p, cx)
}

/// A row whose label can carry a superscript help glyph. The glyph's tooltip
/// is rendered by the settings window so it can escape the card and scroll
/// viewport.
pub fn row_with_help(
    value: impl Into<String>,
    help: Option<AnyElement>,
    control: impl IntoElement,
    p: Palette,
    cx: &gpui::App,
) -> impl IntoElement {
    div()
        .h(px(ROW_HEIGHT))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(20.))
        .px(px(10.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .child(label(value, 13., gpui::FontWeight::NORMAL, p.text, cx))
                .when_some(help, |row, help| row.child(help)),
        )
        .child(control)
}

/// A labelled single-line text control. The input uses the boxless/compact
/// metrics and the row supplies the themed field surface, so the same control
/// works on both theme variants instead of inheriting TextInput's dark border.
pub fn text_row(
    title: impl Into<String>,
    input: gpui::Entity<crate::input::TextInput>,
    width: f32,
    p: Palette,
    cx: &gpui::App,
) -> impl IntoElement {
    row(
        title,
        div()
            .w(px(width))
            .h(px(30.))
            .px(px(8.))
            .rounded(px(7.))
            .border_1()
            .border_color(p.search_border)
            .bg(p.search_bg)
            .flex()
            .items_center()
            .child(input),
        p,
        cx,
    )
}

pub fn stack(rows: Vec<AnyElement>, p: Palette) -> impl IntoElement {
    let mut body = div().flex().flex_col();
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            body = body.child(theme::hairline(p));
        }
        body = body.child(row);
    }
    body
}

pub fn toggle(
    id: &'static str,
    value: bool,
    p: Palette,
    on_change: impl Fn(bool, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    toggle_state(id, value, true, p, on_change)
}

/// Switch with an explicit enabled state. Unsupported values stay visible but
/// cannot be clicked.
pub fn toggle_state(
    id: &'static str,
    value: bool,
    enabled: bool,
    p: Palette,
    on_change: impl Fn(bool, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let fill = if value { p.accent } else { p.switch_off };
    let border = if value { p.accent } else { p.control_border };
    div()
        .id(id)
        .debug_selector(|| id.into())
        .w(px(52.))
        .h(px(30.))
        .flex_none()
        .rounded_full()
        .border_1()
        .border_color(border)
        .bg(fill)
        .p(px(2.))
        .flex()
        .items_center()
        .when(enabled, |div| div.cursor_pointer())
        .when(!enabled, |div| div.opacity(0.42))
        .when(value, |div| div.justify_end())
        .child(
            div()
                .size(px(24.))
                .rounded_full()
                .bg(rgb(0xffffff))
                .shadow(vec![gpui::BoxShadow {
                    color: rgba(0x00000033).into(),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(2.),
                    spread_radius: px(0.),
                }]),
        )
        .when(enabled, |div| {
            div.on_click(move |_, window, cx| on_change(!value, window, cx))
        })
}

pub fn value_row(
    title: &'static str,
    value: impl Into<String>,
    p: Palette,
    cx: &gpui::App,
) -> impl IntoElement {
    row(
        title,
        label(value, 12., gpui::FontWeight::NORMAL, p.muted, cx),
        p,
        cx,
    )
}

#[derive(Default)]
struct SliderGesture {
    dragging: bool,
    preview: Option<(f64, Instant)>,
    last_sent: Option<Instant>,
    last_sent_value: Option<f64>,
}

/// A thin track with a 13-point thumb. Updates are throttled to the same 80 ms
/// cadence as the Shell's own sliders, so dragging cannot flood the socket.
pub fn slider(
    id: &'static str,
    value: f64,
    range: (f64, f64),
    p: Palette,
    on_change: impl Fn(f64, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let normalized = ((value - range.0) / (range.1 - range.0)).clamp(0., 1.);
    let on_change = Rc::new(on_change);
    let key = SharedString::from(format!("settings-slider-{id}"));
    div()
        .id(SharedString::from(format!("settings-slider-row-{id}")))
        .debug_selector(move || format!("slider-{id}"))
        .w(px(280.))
        .h(px(38.))
        .flex_none()
        .cursor_pointer()
        .child(
            gpui::canvas(
                {
                    let key = key.clone();
                    move |bounds, window, cx| {
                        let gesture = window.use_keyed_state(key, cx, |_, _| SliderGesture::default());
                        (gesture, window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal))
                    }
                },
                {
                    let on_change = on_change.clone();
                    move |bounds, (gesture, hitbox), window, cx| {
                        let displayed = gesture.update(cx, |state, _| {
                            state
                                .preview
                                .map(|(value, _)| value)
                                .unwrap_or(normalized)
                                .clamp(0., 1.)
                        });
                        let y = bounds.center().y;
                        let start = bounds.left() + px(8.);
                        let width = (bounds.size.width - px(16.)).max(px(1.));
                        let x = start + width * displayed as f32;
                        let track = Bounds::new(point(start, y - px(2.5)), size(width, px(5.)));
                        window.paint_quad(fill(track, p.switch_off.opacity(0.55)).corner_radii(px(2.5)));
                        let filled = Bounds::new(
                            point(start, y - px(2.5)),
                            size((x - start).max(px(0.)), px(5.)),
                        );
                        window.paint_quad(fill(filled, p.accent).corner_radii(px(2.5)));
                        let thumb = Bounds::new(
                            point(x - px(10.), y - px(10.)),
                            size(px(20.), px(20.)),
                        );
                        window.paint_shadows(
                            thumb,
                            gpui::Corners::all(px(10.)),
                            &[gpui::BoxShadow {
                                color: rgba(0x00000033).into(),
                                offset: point(px(0.), px(1.)),
                                blur_radius: px(3.),
                                spread_radius: px(0.),
                            }],
                        );
                        window.paint_quad(
                            fill(thumb, rgb(0xffffff))
                                .corner_radii(px(10.))
                                .border_widths(px(0.5))
                                .border_color(rgba(0x00000026)),
                        );

                        let down_gesture = gesture.clone();
                        let down_change = on_change.clone();
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase != gpui::DispatchPhase::Bubble
                                || event.button != MouseButton::Left
                                || !hitbox.is_hovered(window)
                            {
                                return;
                            }
                            down_gesture.update(cx, |state, _| state.dragging = true);
                            let value = slider_value(bounds, event.position.x);
                            down_gesture.update(cx, |state, _| state.preview = Some((value, Instant::now())));
                            down_change(range.0 + (range.1 - range.0) * value, window, cx);
                            window.refresh();
                            cx.stop_propagation();
                        });
                        let move_gesture = gesture.clone();
                        let move_change = on_change.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase != gpui::DispatchPhase::Bubble
                                || !move_gesture.read(cx).dragging
                            {
                                return;
                            }
                            if event.pressed_button != Some(MouseButton::Left) {
                                move_gesture.update(cx, |state, _| state.dragging = false);
                                return;
                            }
                            let value = slider_value(bounds, event.position.x);
                            let send = move_gesture.update(cx, |state, _| {
                                let now = Instant::now();
                                state.preview = Some((value, now));
                                let send = state.last_sent_value != Some(value)
                                    && state.last_sent.is_none_or(|sent| {
                                        sent.elapsed() >= Duration::from_millis(80)
                                    });
                                if send {
                                    state.last_sent = Some(now);
                                    state.last_sent_value = Some(value);
                                }
                                send
                            });
                            if send {
                                move_change(range.0 + (range.1 - range.0) * value, window, cx);
                            }
                            window.refresh();
                        });
                        let up_gesture = gesture.clone();
                        let up_change = on_change.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase != gpui::DispatchPhase::Bubble
                                || event.button != MouseButton::Left
                                || !up_gesture.read(cx).dragging
                            {
                                return;
                            }
                            let value = slider_value(bounds, event.position.x);
                            up_gesture.update(cx, |state, _| {
                                state.dragging = false;
                                state.preview = Some((value, Instant::now()));
                            });
                            up_change(range.0 + (range.1 - range.0) * value, window, cx);
                            window.refresh();
                            cx.stop_propagation();
                        });
                    }
                },
            )
            .size_full(),
        )
}

fn slider_value(bounds: Bounds<gpui::Pixels>, x: gpui::Pixels) -> f64 {
    let width = f32::from((bounds.size.width - px(16.)).max(px(1.)));
    f64::from((f32::from(x - bounds.left() - px(8.)) / width).clamp(0., 1.))
}

/// Small rounded action button used by the live desktop pages.
pub fn button(
    id: impl Into<SharedString>,
    title: impl Into<String>,
    p: Palette,
    cx: &gpui::App,
    on_click: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let id = id.into();
    let debug = id.clone();
    div()
        .id(id)
        .debug_selector(move || debug.to_string())
        .h(px(30.))
        .px(px(12.))
        .flex()
        .items_center()
        .rounded(px(8.))
        .border_1()
        .border_color(p.control_border)
        .bg(p.control)
        .cursor_pointer()
        .hover(move |div| div.bg(p.chip_hover))
        .child(label(
            title,
            11.,
            gpui::FontWeight::MEDIUM,
            p.text,
            cx,
        ))
        .on_click(move |_, window, cx| {
            on_click(window, cx);
            cx.stop_propagation();
        })
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
pub fn slider_row(
    title: &'static str,
    id: &'static str,
    value: f64,
    range: (f64, f64),
    display: String,
    p: Palette,
    cx: &gpui::App,
    on_change: impl Fn(f64, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    row(
        title,
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(slider(id, value, range, p, on_change))
            .child(
                div()
                    .w(px(SLIDER_VALUE_WIDTH))
                    .text_right()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(label(
                        display,
                        11.,
                        gpui::FontWeight::NORMAL,
                        p.muted,
                        cx,
                    )),
            ),
        p,
        cx,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn slider_row_with_help(
    title: &'static str,
    id: &'static str,
    value: f64,
    range: (f64, f64),
    display: String,
    help: Option<AnyElement>,
    p: Palette,
    cx: &gpui::App,
    on_change: impl Fn(f64, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    row_with_help(
        title,
        help,
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(slider(id, value, range, p, on_change))
            .child(
                div()
                    .w(px(SLIDER_VALUE_WIDTH))
                    .text_right()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(label(
                        display,
                        11.,
                        gpui::FontWeight::NORMAL,
                        p.muted,
                        cx,
                    )),
            ),
        p,
        cx,
    )
}

/// Static slider used by disabled rows: same geometry, no interaction.
fn slider_static(value: f64, range: (f64, f64), p: Palette) -> impl IntoElement {
    let normalized = ((value - range.0) / (range.1 - range.0)).clamp(0., 1.);
    div()
        .w(px(280.))
        .h(px(38.))
        .flex_none()
        .opacity(0.45)
        .child(
            gpui::canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let y = bounds.center().y;
                    let start = bounds.left() + px(8.);
                    let width = (bounds.size.width - px(16.)).max(px(1.));
                    let x = start + width * normalized as f32;
                    let track = Bounds::new(point(start, y - px(2.5)), size(width, px(5.)));
                    window.paint_quad(
                        fill(track, p.switch_off.opacity(0.55)).corner_radii(px(2.5)),
                    );
                    let filled = Bounds::new(
                        point(start, y - px(2.5)),
                        size((x - start).max(px(0.)), px(5.)),
                    );
                    window.paint_quad(fill(filled, p.muted.opacity(0.55)).corner_radii(px(2.5)));
                    let thumb =
                        Bounds::new(point(x - px(10.), y - px(10.)), size(px(20.), px(20.)));
                    window.paint_quad(fill(thumb, p.muted.opacity(0.8)).corner_radii(px(10.)));
                },
            )
            .size_full(),
        )
}

#[allow(clippy::too_many_arguments)]
pub fn slider_row_state(
    title: &'static str,
    id: &'static str,
    value: f64,
    range: (f64, f64),
    display: String,
    enabled: bool,
    p: Palette,
    cx: &gpui::App,
    on_change: impl Fn(f64, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    if enabled {
        return row(
            title,
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(slider(id, value, range, p, on_change))
                .child(
                    div()
                        .w(px(SLIDER_VALUE_WIDTH))
                        .text_right()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(label(
                    display,
                    11.,
                    gpui::FontWeight::NORMAL,
                    p.muted,
                    cx,
                    )),
                ),
            p,
            cx,
        )
        .into_any_element();
    }
    row(
        title,
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(slider_static(value, range, p))
            .child(
                div()
                    .w(px(SLIDER_VALUE_WIDTH))
                    .text_right()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(label(
                        display,
                        11.,
                        gpui::FontWeight::NORMAL,
                        p.muted.opacity(0.6),
                        cx,
                    )),
            ),
        p,
        cx,
    )
    .into_any_element()
}

/// A round colour control. The ring is the click target; the inner disc paints
/// the selected colour (or an empty marker for an optional "inherit" value).
/// Superscript help glyph (`help-hint-symbolic` shape). The caller renders the
/// tooltip so it can float above cards and the scroll viewport.
/// Rainbow-ish circular button that opens the custom accent picker.
pub fn custom_color_swatch(
    id: &'static str,
    p: Palette,
    on_click: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .size(px(34.))
        .rounded_full()
        .border_1()
        .border_color(p.control_border)
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |div| div.bg(p.hover))
        .child(
            div()
                .size(px(26.))
                .rounded_full()
                .border_1()
                .border_color(rgba(0xffffff40))
                .bg(linear_gradient(
                    45.,
                    linear_color_stop(rgb(0xff375f), 0.),
                    linear_color_stop(rgb(0x0a84ff), 1.),
                ))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div().size(px(14.)).flex_none().child(
                        gpui::canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                let center = bounds.center();
                                let half = 5.;
                                let thickness = 2.;
                                for (width, height) in
                                    [(half * 2., thickness), (thickness, half * 2.)]
                                {
                                    window.paint_quad(
                                        fill(
                                            Bounds::new(
                                                point(
                                                    center.x - px(width / 2.),
                                                    center.y - px(height / 2.),
                                                ),
                                                size(px(width), px(height)),
                                            ),
                                            rgb(0xffffff),
                                        )
                                        .corner_radii(px(1.)),
                                    );
                                }
                            },
                        )
                        .size_full(),
                    ),
                ),
        )
        .on_click(move |_, window, cx| on_click(window, cx))
        .into_any_element()
}

pub fn help_hint(
    id: &'static str,
    p: Palette,
    on_hover: impl Fn(bool, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || format!("help-{id}"))
        .relative()
        .top(px(-5.))
        .size(px(15.))
        .flex_none()
        .cursor_pointer()
        .opacity(0.62)
        .hover(move |div| div.opacity(1.))
        .child(
            gpui::canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let center = bounds.center();
                    let radius = f32::from(bounds.size.width.min(bounds.size.height)) / 2. - 1.;
                    let mut path = PathBuilder::stroke(px(1.3));
                    for index in 0..=40 {
                        let angle = -std::f32::consts::FRAC_PI_2
                            + std::f32::consts::TAU * index as f32 / 40.;
                        let point = point(
                            center.x + px(radius * angle.cos()),
                            center.y + px(radius * angle.sin()),
                        );
                        if index == 0 {
                            path.move_to(point);
                        } else {
                            path.line_to(point);
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, p.muted);
                    }
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                point(center.x - px(1.1), center.y - px(4.7)),
                                size(px(2.2), px(2.2)),
                            ),
                            p.muted,
                        )
                        .corner_radii(px(1.1)),
                    );
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                point(center.x - px(1.1), center.y - px(1.9)),
                                size(px(2.2), px(5.5)),
                            ),
                            p.muted,
                        )
                        .corner_radii(px(1.1)),
                    );
                },
            )
            .size_full(),
        )
        .on_hover(move |hovered, window, cx| on_hover(*hovered, window, cx))
        .into_any_element()
}

#[derive(Default)]
struct DialGesture {
    dragging: bool,
    preview: Option<(f64, Instant)>,
    last_sent: Option<Instant>,
    last_sent_value: Option<f64>,
}

/// Ring-shaped value control. Dragging around the ring sets `fraction`
/// (0..1); the centre shows the caller-formatted value.
pub fn dial(
    id: &'static str,
    fraction: f64,
    display: impl Into<String>,
    p: Palette,
    cx: &gpui::App,
    on_change: impl Fn(f64, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let normalized = fraction.clamp(0., 1.);
    let display = display.into();
    let on_change = Rc::new(on_change);
    let key = SharedString::from(format!("settings-dial-{id}"));
    div()
        .id(id)
        .debug_selector(move || format!("dial-{id}"))
        .relative()
        .size(px(58.))
        .flex_none()
        .cursor_pointer()
        .child(
            gpui::canvas(
                {
                    let key = key.clone();
                    move |bounds, window, cx| {
                        let gesture =
                            window.use_keyed_state(key, cx, |_, _| DialGesture::default());
                        (
                            gesture,
                            window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
                        )
                    }
                },
                {
                    let on_change = on_change.clone();
                    move |bounds, (gesture, hitbox), window, cx| {
                        let displayed = gesture.update(cx, |state, _| {
                            state
                                .preview
                                .map(|(value, _)| value)
                                .unwrap_or(normalized)
                                .clamp(0., 1.)
                        });
                        let center = bounds.center();
                        let radius =
                            f32::from(bounds.size.width.min(bounds.size.height)) / 2. - 5.;
                        let paint_arc = |start: f32, sweep: f32, width: f32, color: gpui::Hsla, window: &mut Window| {
                            if sweep.abs() < 0.0001 {
                                return;
                            }
                            let steps = ((sweep.abs() / std::f32::consts::TAU) * 48.).ceil().max(2.);
                            let mut path = PathBuilder::stroke(px(width));
                            for index in 0..=steps as usize {
                                let angle = start + sweep * index as f32 / steps;
                                let point = point(
                                    center.x + px(radius * angle.cos()),
                                    center.y + px(radius * angle.sin()),
                                );
                                if index == 0 {
                                    path.move_to(point);
                                } else {
                                    path.line_to(point);
                                }
                            }
                            if let Ok(path) = path.build() {
                                window.paint_path(path, color);
                            }
                        };
                        paint_arc(
                            -std::f32::consts::FRAC_PI_2,
                            std::f32::consts::TAU,
                            4.,
                            p.switch_off.opacity(0.55),
                            window,
                        );
                        let sweep = std::f32::consts::TAU * displayed as f32;
                        paint_arc(-std::f32::consts::FRAC_PI_2, sweep, 4., p.accent, window);
                        if displayed > 0.002 {
                            let angle = -std::f32::consts::FRAC_PI_2 + sweep;
                            let knob = Bounds::new(
                                point(
                                    center.x + px(radius * angle.cos()) - px(5.),
                                    center.y + px(radius * angle.sin()) - px(5.),
                                ),
                                size(px(10.), px(10.)),
                            );
                            window.paint_quad(fill(knob, p.accent).corner_radii(px(5.)));
                        }

                        let down_gesture = gesture.clone();
                        let down_change = on_change.clone();
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase != gpui::DispatchPhase::Bubble
                                || event.button != MouseButton::Left
                                || !hitbox.is_hovered(window)
                            {
                                return;
                            }
                            down_gesture.update(cx, |state, _| state.dragging = true);
                            let value = dial_fraction(bounds, event.position);
                            down_gesture.update(cx, |state, _| {
                                state.preview = Some((value, Instant::now()))
                            });
                            down_change(value, window, cx);
                            window.refresh();
                            cx.stop_propagation();
                        });
                        let move_gesture = gesture.clone();
                        let move_change = on_change.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase != gpui::DispatchPhase::Bubble
                                || !move_gesture.read(cx).dragging
                            {
                                return;
                            }
                            if event.pressed_button != Some(MouseButton::Left) {
                                move_gesture.update(cx, |state, _| state.dragging = false);
                                return;
                            }
                            let value = dial_fraction(bounds, event.position);
                            let send = move_gesture.update(cx, |state, _| {
                                let now = Instant::now();
                                state.preview = Some((value, now));
                                let send = state.last_sent_value != Some(value)
                                    && state.last_sent.is_none_or(|sent| {
                                        sent.elapsed() >= Duration::from_millis(60)
                                    });
                                if send {
                                    state.last_sent = Some(now);
                                    state.last_sent_value = Some(value);
                                }
                                send
                            });
                            if send {
                                move_change(value, window, cx);
                            }
                            window.refresh();
                        });
                        let up_gesture = gesture.clone();
                        let up_change = on_change.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase != gpui::DispatchPhase::Bubble
                                || event.button != MouseButton::Left
                                || !up_gesture.read(cx).dragging
                            {
                                return;
                            }
                            let value = dial_fraction(bounds, event.position);
                            up_gesture.update(cx, |state, _| {
                                state.dragging = false;
                                state.preview = Some((value, Instant::now()));
                            });
                            up_change(value, window, cx);
                            window.refresh();
                            cx.stop_propagation();
                        });
                    }
                },
            )
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(label(
                    display,
                    10.5,
                    gpui::FontWeight::MEDIUM,
                    p.text,
                    cx,
                )),
        )
        .into_any_element()
}

fn dial_fraction(bounds: Bounds<gpui::Pixels>, position: gpui::Point<gpui::Pixels>) -> f64 {
    let center = bounds.center();
    let dx = f32::from(position.x - center.x);
    let dy = f32::from(position.y - center.y);
    if dx.hypot(dy) < 3. {
        return 0.;
    }
    let degrees = (dx.atan2(-dy).to_degrees() + 360.) % 360.;
    f64::from(degrees / 360.)
}

pub fn color_swatch(
    id: &'static str,
    color: gpui::Hsla,
    empty: bool,
    p: Palette,
    on_click: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .size(px(34.))
        .rounded_full()
        .border_1()
        .border_color(p.control_border)
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |div| div.bg(p.hover))
        .child(
            div()
                .size(px(26.))
                .rounded_full()
                .border_1()
                .border_color(if empty {
                    p.muted.opacity(0.45)
                } else {
                    p.text.opacity(0.22)
                })
                .bg(if empty {
                    p.text.opacity(0.05)
                } else {
                    color
                }),
        )
        .on_click(move |_, window, cx| on_click(window, cx))
        .into_any_element()
}

pub fn sidebar_row(
    id: SharedString,
    item: &'static crate::nav::NavItem,
    selected: bool,
    p: Palette,
    cx: &gpui::App,
    scale_factor: f32,
    on_click: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.clone())
        .debug_selector({
            let id = id.clone();
            move || id.to_string()
        })
        .h(px(theme::SIDEBAR_ITEM))
        .w_full()
        .flex()
        .items_center()
        .gap(px(14.))
        .px(px(14.))
        .rounded(px(10.))
        .cursor_pointer()
        .when(selected, |div| div.bg(p.accent))
        .when(!selected, |div| div.hover(move |div| div.bg(p.hover)))
        .child(crate::icons::element(item.icon, 30., scale_factor))
        .child(label(
            item.title,
            13.,
            if selected {
                gpui::FontWeight::MEDIUM
            } else {
                gpui::FontWeight::NORMAL
            },
            if selected { p.on_accent } else { p.text },
            cx,
        ))
        .on_click(move |_, window, cx| on_click(window, cx))
}

pub fn group_heading(title: &'static str, p: Palette, cx: &gpui::App) -> impl IntoElement {
    div()
        .h(px(32.))
        .w_full()
        .flex()
        .items_end()
        .px(px(14.))
        .child(label(title, 11., gpui::FontWeight::MEDIUM, p.muted, cx))
}

#[allow(clippy::too_many_arguments)]
pub fn theme_card(
    id: &'static str,
    title: &'static str,
    dark: bool,
    selected: bool,
    wallpaper: Option<Arc<Image>>,
    accent: gpui::Hsla,
    p: Palette,
    cx: &gpui::App,
    on_click: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let desk = if dark { rgb(0x2c2c31) } else { rgb(0xf2f2f5) };
    let chrome = if dark { rgb(0x3b3b42) } else { rgb(0xffffff) };
    let mut preview = div()
        .w(px(180.))
        .h(px(108.))
        .rounded(px(12.))
        .bg(desk)
        .relative()
        .overflow_hidden();
    if let Some(image) = wallpaper {
        preview = preview.child(
            div().absolute().inset_0().child(
                img(ImageSource::Image(image))
                    .object_fit(ObjectFit::Fill)
                    .size_full(),
            ),
        );
    }
    preview = preview
        .child(
            div()
                .absolute()
                .left(px(12.))
                .top(px(10.))
                .w(px(60.))
                .h(px(34.))
                .rounded(px(4.))
                .bg(chrome)
                .border_1()
                .border_color(rgba(0x00000018))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(17.))
                        .bg(accent.opacity(0.92)),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(24.))
                        .top(px(8.))
                        .w(px(36.))
                        .h(px(5.))
                        .rounded(px(2.5))
                        .bg(if dark {
                            rgba(0xffffff2e)
                        } else {
                            rgba(0x00000022)
                        }),
                ),
        )
        .child(
            div()
                .absolute()
                .left(px(31.5))
                .bottom(px(7.))
                .h(px(12.))
                .w(px(117.))
                .rounded(px(6.))
                .bg(accent.opacity(0.42)),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(12.))
                .border_2()
                .border_color(if selected { p.accent } else { p.card_border }),
        );
    div()
        .id(id)
        .debug_selector(|| id.into())
        .flex()
        .flex_col()
        .items_center()
        .gap(px(5.))
        .cursor_pointer()
        .child(preview)
        .child(label(
            title,
            11.,
            gpui::FontWeight::NORMAL,
            p.text,
            cx,
        ))
        .on_click(move |_, window, cx| on_click(window, cx))
}

pub fn swatch(
    id: SharedString,
    value: &'static str,
    selected: bool,
    p: Palette,
    on_click: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let color = value
        .strip_prefix('#')
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .unwrap_or(0x0a84ff);
    div()
        .id(id.clone())
        .debug_selector({
            let id = id.clone();
            move || id.to_string()
        })
        .size(px(42.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_2()
        .border_color(if selected { p.accent } else { gpui::Hsla::transparent_black() })
        .cursor_pointer()
        .child(div().size(px(34.)).rounded_full().bg(rgb(color)))
        .on_click(move |_, window, cx| on_click(window, cx))
}


/// A small vector magnifier for the sidebar search field.
pub fn magnifier(p: Palette, size: f32) -> AnyElement {
    div()
        .size(px(size))
        .flex_none()
        .child(
            gpui::svg()
                .path("search.svg")
                .size(px(size))
                .text_color(p.muted),
        )
        .into_any_element()
}


/// A vector chevron for the back/forward capsule. Drawing it removes the
/// font-glyph variance that made the previous pair look undersized.
pub fn chevron(
    right: bool,
    size: f32,
    color: gpui::Hsla,
) -> AnyElement {
    div()
        .size(px(size))
        .flex_none()
        .child(gpui::canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let center = bounds.center();
                let half = size * 0.29;
                let reach = size * 0.26;
                let direction = if right { 1. } else { -1. };
                let stroke = (size * 0.115).max(2.);
                let radius = stroke / 2.;
                let top = point(center.x - px(reach * direction), center.y - px(half));
                let tip = point(center.x + px(reach * direction), center.y);
                let bottom = point(center.x - px(reach * direction), center.y + px(half));
                let mut path = PathBuilder::stroke(px(stroke));
                path.move_to(top);
                path.line_to(tip);
                path.line_to(bottom);
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
                // Round the joins and ends: the reference arrows are soft, not
                // a sharp angle produced by a bare three-point stroke.
                for join in [top, tip, bottom] {
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                gpui::point(join.x - px(radius), join.y - px(radius)),
                                gpui::size(px(stroke), px(stroke)),
                            ),
                            color,
                        )
                        .corner_radii(px(radius)),
                    );
                }
            },
        )
        .size_full())
        .into_any_element()
}


pub fn chevron_up_down(color: gpui::Hsla, size: f32) -> AnyElement {
    div()
        .size(px(size))
        .flex_none()
        .child(gpui::canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let center = bounds.center();
                let half = size * 0.18;
                let wide = size * 0.28;
                let stroke = (size * 0.11).max(1.6);
                let radius = stroke / 2.;
                let up = [
                    point(center.x - px(wide), center.y - px(half)),
                    point(center.x, center.y - px(half * 2.4)),
                    point(center.x + px(wide), center.y - px(half)),
                ];
                let down = [
                    point(center.x - px(wide), center.y + px(half)),
                    point(center.x, center.y + px(half * 2.4)),
                    point(center.x + px(wide), center.y + px(half)),
                ];
                let mut path = PathBuilder::stroke(px(stroke));
                path.move_to(up[0]);
                path.line_to(up[1]);
                path.line_to(up[2]);
                path.move_to(down[0]);
                path.line_to(down[1]);
                path.line_to(down[2]);
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
                for join in up.into_iter().chain(down) {
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                gpui::point(join.x - px(radius), join.y - px(radius)),
                                gpui::size(px(stroke), px(stroke)),
                            ),
                            color,
                        )
                        .corner_radii(px(radius)),
                    );
                }
            },
        )
        .size_full())
        .into_any_element()
}
