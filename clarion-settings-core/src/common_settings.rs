//! Typed controls backed by the compositor's effective KDL configuration.
//!
//! Only changed fields are written. Whole-group overrides first copy their
//! effective source, and the merged candidate must equal the intended config.
use std::path::PathBuf;

use anyhow::{Context, Result};
use clarion_config::animations::{Animation, Animations, Curve, Kind, WindowOpenEffect};
use std::str::FromStr;

use clarion_config::{Config, IdleAction, PresetSize, WindowCloseEffect};
use knuffel::ast::SpannedNode;
use knuffel::span::Span;

use crate::config_manager::ConfigManager;
use crate::kdl_edit::Editor;

#[derive(Clone, Debug, PartialEq)]
pub struct CommonSettings {
    pub animations_enabled: bool,
    /// The compositor's duration multiplier: smaller means faster.
    pub animation_speed: f64,
    pub window_open_enabled: bool,
    pub window_close_enabled: bool,
    pub window_movement_enabled: bool,
    pub window_resize_enabled: bool,
    pub workspace_switch_enabled: bool,
    pub window_open_effect: String,
    pub window_close_effect: String,
    /// Read from the effective shader; true -> false is explicit replacement.
    pub open_custom_shader: bool,
    pub close_custom_shader: bool,
    pub wobble_enabled: bool,
    pub cube_enabled: bool,
    pub glide_enabled: bool,
    pub auto_stack: bool,
    pub max_windows_per_column: u32,
    pub default_width: Option<PresetSize>,
    pub gaps: f64,
    pub center_column: String,
    pub center_single: bool,
    pub border_enabled: bool,
    pub focus_ring_enabled: bool,
    pub shadow_enabled: bool,
    pub border_width: f64,
    /// Rotation of the active border gradient; inactive/urgent stay independent.
    pub border_gradient_enabled: bool,
    pub border_gradient_speed: f64,
    pub border_gradient_from: String,
    pub border_gradient_to: String,
    pub border_gradient_angle: f64,
    pub border_gradient_relative: String,
    pub border_gradient_in: String,
    pub focus_ring_width: f64,
    pub rounding: f64,
    pub blur_enabled: bool,
    pub blur_passes: u8,
    pub blur_offset: f64,
    pub touchpad_tap: bool,
    pub touchpad_natural_scroll: bool,
    pub touchpad_dwt: bool,
    pub touchpad_speed: f64,
    pub touchpad_scroll_factor: f64,
    pub mouse_natural_scroll: bool,
    pub mouse_speed: f64,
    pub keyboard_numlock: bool,
    pub keyboard_repeat_rate: u8,
    pub keyboard_repeat_delay: u16,
    pub keyboard_xkb_layout: String,
    pub keyboard_xkb_variant: String,
    pub keyboard_xkb_options: String,
    pub touchpad_drag: bool,
    pub touchpad_drag_lock: bool,
    pub touchpad_click_method: String,
    pub touchpad_accel_profile: String,
    pub touchpad_scroll_method: String,
    pub touchpad_tap_button_map: String,
    pub touchpad_left_handed: bool,
    pub touchpad_middle_emulation: bool,
    pub touchpad_disable_on_external_mouse: bool,
    pub touchpad_dwtp: bool,
    pub mouse_accel_profile: String,
    pub mouse_scroll_method: String,
    pub mouse_left_handed: bool,
    pub mouse_middle_emulation: bool,
    pub trackpoint_enabled: bool,
    pub trackpoint_natural_scroll: bool,
    pub trackpoint_speed: f64,
    pub trackpoint_accel_profile: String,
    pub trackpoint_left_handed: bool,
    pub trackpoint_middle_emulation: bool,
    pub screenshot_path: String,
    pub xwayland_enabled: bool,
    pub xwayland_path: String,
    pub prefer_no_csd: bool,
    pub clipboard_disable_primary: bool,
    pub hotkey_skip_at_startup: bool,
    pub hotkey_hide_not_bound: bool,
    pub config_notification_disable_failed: bool,
    pub idle_enabled: bool,
    pub idle_timeout: u32,
    pub idle_blank_timeout: u32,
    pub idle_action: String,
    pub idle_lock_command: String,
    pub idle_lock_before_suspend: bool,
    pub cursor_theme: String,
    pub cursor_size: u8,
    pub cursor_hide_when_typing: bool,
    pub cursor_hide_after_ms: Option<u32>,
    pub hot_corners_enabled: bool,
    pub hot_corner_top_left: bool,
    pub hot_corner_top_right: bool,
    pub hot_corner_bottom_left: bool,
    pub hot_corner_bottom_right: bool,
    pub hot_corner_top_left_action: String,
    pub hot_corner_top_right_action: String,
    pub hot_corner_bottom_left_action: String,
    pub hot_corner_bottom_right_action: String,
    pub blur_noise: f64,
    pub blur_saturation: f64,
    pub border_active_color: String,
    pub border_inactive_color: String,
    pub border_urgent_color: String,
    pub focus_ring_active_color: String,
    pub focus_ring_inactive_color: String,
    pub focus_ring_urgent_color: String,
    pub shadow_color: String,
    pub shadow_inactive_color: String,
    pub shadow_offset_x: f64,
    pub shadow_offset_y: f64,
    pub shadow_softness: f64,
    pub shadow_spread: f64,
    pub shadow_draw_behind: bool,
    pub struts_left: f64,
    pub struts_right: f64,
    pub struts_top: f64,
    pub struts_bottom: f64,
    pub dim_inactive: bool,
    pub dim_factor: f64,
    pub layout_background_color: String,
    pub layout_fullscreen_backdrop_color: String,
    pub rounding_power: f64,
    pub tab_indicator_enabled: bool,
    pub tab_indicator_hide_single: bool,
    pub tab_indicator_gap: f64,
    pub tab_indicator_width: f64,
    pub tab_indicator_corner_radius: f64,
    pub tab_indicator_active_color: String,
    pub tab_indicator_inactive_color: String,
    pub tab_indicator_urgent_color: String,
    pub insert_hint_enabled: bool,
    pub insert_hint_color: String,
    pub overview_style: String,
    pub overview_zoom: f64,
    pub overview_backdrop_color: String,
    pub stage_manager_enabled: bool,
    pub overview_shadow_enabled: bool,
    pub overview_shadow_offset_x: f64,
    pub overview_shadow_offset_y: f64,
    pub overview_shadow_softness: f64,
    pub overview_shadow_spread: f64,
    pub overview_shadow_color: String,
    pub preset_column_widths: String,
    pub preset_window_heights: String,
}

impl Default for CommonSettings {
    fn default() -> Self {
        Self::from_config(&Config::default())
    }
}

impl CommonSettings {
    pub fn read(manager: &ConfigManager) -> Result<Self> {
        read(manager)
    }

    pub fn apply(manager: &mut ConfigManager, before: &Self, after: &Self) -> Result<()> {
        apply(manager, before, after)
    }

    fn from_config(config: &Config) -> Self {
        let a = &config.animations;
        let l = &config.layout;
        let t = &config.input.touchpad;
        let m = &config.input.mouse;
        let k = &config.input.keyboard;
        Self {
            animations_enabled: !a.off,
            animation_speed: a.slowdown,
            window_open_enabled: !a.window_open.anim.off,
            window_close_enabled: !a.window_close.anim.off,
            window_movement_enabled: !a.window_movement.0.off,
            window_resize_enabled: !a.window_resize.anim.off,
            workspace_switch_enabled: !a.workspace_switch.0.off,
            window_open_effect: match a.window_open.effect {
                WindowOpenEffect::Fade => "fade",
                WindowOpenEffect::Reveal => "reveal",
            }
            .into(),
            window_close_effect: match a.window_close.effect {
                WindowCloseEffect::Explosion => "explosion",
                WindowCloseEffect::Implode => "implode",
                WindowCloseEffect::Fade => "fade",
            }
            .into(),
            open_custom_shader: a.window_open.custom_shader.is_some(),
            close_custom_shader: a.window_close.custom_shader.is_some(),
            wobble_enabled: !a.window_wobble.off,
            cube_enabled: !a.workspace_switch.1.off,
            // Opening and closing have independent Glide settings. This control
            // changes opening only, preserving independently tuned closing.
            glide_enabled: !a.window_open.glide.off,
            auto_stack: l.auto_stack_new_windows,
            max_windows_per_column: l.max_windows_per_column,
            default_width: l.default_column_width,
            gaps: l.gaps,
            center_column: match l.center_focused_column {
                clarion_config::layout::CenterFocusedColumn::Never => "never",
                clarion_config::layout::CenterFocusedColumn::Always => "always",
                clarion_config::layout::CenterFocusedColumn::OnOverflow => "on-overflow",
            }
            .into(),
            center_single: l.always_center_single_column,
            border_enabled: !l.border.off,
            focus_ring_enabled: !l.focus_ring.off,
            shadow_enabled: l.shadow.on,
            border_width: l.border.width,
            border_gradient_enabled: l.border.active_gradient.is_some(),
            border_gradient_speed: l
                .border
                .active_gradient
                .and_then(|gradient| gradient.rotate_deg_per_sec)
                .map_or(0., |speed| speed.0),
            border_gradient_from: color_text(
                l.border
                    .active_gradient
                    .map_or(l.border.active_color, |gradient| gradient.from),
            ),
            border_gradient_to: color_text(
                l.border
                    .active_gradient
                    .map_or(l.border.inactive_color, |gradient| gradient.to),
            ),
            border_gradient_angle: l
                .border
                .active_gradient
                .map_or(180., |gradient| f64::from(gradient.angle)),
            border_gradient_relative: relative_text(
                l.border
                    .active_gradient
                    .map_or(clarion_config::appearance::GradientRelativeTo::Window, |gradient| {
                        gradient.relative_to
                    }),
            )
            .into(),
            border_gradient_in: l.border.active_gradient.map_or_else(String::new, |gradient| {
                interpolation_text(gradient.in_)
            }),
            focus_ring_width: l.focus_ring.width,
            rounding: l.rounding,
            blur_enabled: !config.blur.off,
            blur_passes: config.blur.passes,
            blur_offset: config.blur.offset,
            touchpad_tap: t.tap,
            touchpad_natural_scroll: t.natural_scroll,
            touchpad_dwt: t.dwt,
            touchpad_speed: t.accel_speed.0,
            touchpad_scroll_factor: t.scroll_factor.and_then(|f| f.base).map_or(1., |f| f.0),
            mouse_natural_scroll: m.natural_scroll,
            mouse_speed: m.accel_speed.0,
            keyboard_numlock: k.numlock,
            keyboard_repeat_rate: k.repeat_rate,
            keyboard_repeat_delay: k.repeat_delay,
            keyboard_xkb_layout: k.xkb.layout.clone(),
            keyboard_xkb_variant: k.xkb.variant.clone(),
            keyboard_xkb_options: k.xkb.options.clone().unwrap_or_default(),
            touchpad_drag: t.drag.unwrap_or(false),
            touchpad_drag_lock: t.drag_lock,
            touchpad_click_method: click_method_text(t.click_method),
            touchpad_accel_profile: accel_profile_text(t.accel_profile),
            touchpad_scroll_method: scroll_method_text(t.scroll_method),
            touchpad_tap_button_map: tap_button_map_text(t.tap_button_map),
            touchpad_left_handed: t.left_handed,
            touchpad_middle_emulation: t.middle_emulation,
            touchpad_disable_on_external_mouse: t.disabled_on_external_mouse,
            touchpad_dwtp: t.dwtp,
            mouse_accel_profile: accel_profile_text(m.accel_profile),
            mouse_scroll_method: scroll_method_text(m.scroll_method),
            mouse_left_handed: m.left_handed,
            mouse_middle_emulation: m.middle_emulation,
            trackpoint_enabled: !config.input.trackpoint.off,
            trackpoint_natural_scroll: config.input.trackpoint.natural_scroll,
            trackpoint_speed: config.input.trackpoint.accel_speed.0,
            trackpoint_accel_profile: accel_profile_text(
                config.input.trackpoint.accel_profile,
            ),
            trackpoint_left_handed: config.input.trackpoint.left_handed,
            trackpoint_middle_emulation: config.input.trackpoint.middle_emulation,
            screenshot_path: config.screenshot_path.0.clone().unwrap_or_default(),
            xwayland_enabled: !config.xwayland_satellite.off,
            xwayland_path: config.xwayland_satellite.path.clone(),
            prefer_no_csd: config.prefer_no_csd,
            clipboard_disable_primary: config.clipboard.disable_primary,
            hotkey_skip_at_startup: config.hotkey_overlay.skip_at_startup,
            hotkey_hide_not_bound: config.hotkey_overlay.hide_not_bound,
            config_notification_disable_failed: config.config_notification.disable_failed,
            idle_enabled: !config.idle.off,
            idle_timeout: config.idle.timeout,
            idle_blank_timeout: config.idle.blank_timeout,
            idle_action: match config.idle.action {
                IdleAction::None => "none",
                IdleAction::Lock => "lock",
                IdleAction::Suspend => "suspend",
            }
            .into(),
            idle_lock_command: config.idle.lock_command.clone().unwrap_or_default(),
            idle_lock_before_suspend: config.idle.lock_before_suspend,
            cursor_theme: config.cursor.xcursor_theme.clone(),
            cursor_size: config.cursor.xcursor_size,
            cursor_hide_when_typing: config.cursor.hide_when_typing,
            cursor_hide_after_ms: config.cursor.hide_after_inactive_ms,
            hot_corners_enabled: !config.gestures.hot_corners.off,
            hot_corner_top_left: config.gestures.hot_corners.top_left,
            hot_corner_top_right: config.gestures.hot_corners.top_right,
            hot_corner_bottom_left: config.gestures.hot_corners.bottom_left,
            hot_corner_bottom_right: config.gestures.hot_corners.bottom_right,
            hot_corner_top_left_action: hot_corner_action_key(
                config
                    .gestures
                    .hot_corners
                    .top_left_action
                    .unwrap_or(clarion_config::gestures::HotCornerAction::Overview),
            ),
            hot_corner_top_right_action: hot_corner_action_key(
                config
                    .gestures
                    .hot_corners
                    .top_right_action
                    .unwrap_or(clarion_config::gestures::HotCornerAction::Expo),
            ),
            hot_corner_bottom_left_action: hot_corner_action_key(
                config
                    .gestures
                    .hot_corners
                    .bottom_left_action
                    .unwrap_or(clarion_config::gestures::HotCornerAction::None),
            ),
            hot_corner_bottom_right_action: hot_corner_action_key(
                config
                    .gestures
                    .hot_corners
                    .bottom_right_action
                    .unwrap_or(clarion_config::gestures::HotCornerAction::Overview),
            ),
            blur_noise: config.blur.noise,
            blur_saturation: config.blur.saturation,
            border_active_color: color_text(l.border.active_color),
            border_inactive_color: color_text(l.border.inactive_color),
            border_urgent_color: color_text(l.border.urgent_color),
            focus_ring_active_color: color_text(l.focus_ring.active_color),
            focus_ring_inactive_color: color_text(l.focus_ring.inactive_color),
            focus_ring_urgent_color: color_text(l.focus_ring.urgent_color),
            shadow_color: color_text(l.shadow.color),
            shadow_inactive_color: l
                .shadow
                .inactive_color
                .map(color_text)
                .unwrap_or_default(),
            shadow_offset_x: l.shadow.offset.x.0,
            shadow_offset_y: l.shadow.offset.y.0,
            shadow_softness: l.shadow.softness,
            shadow_spread: l.shadow.spread,
            shadow_draw_behind: l.shadow.draw_behind_window,
            struts_left: l.struts.left.0,
            struts_right: l.struts.right.0,
            struts_top: l.struts.top.0,
            struts_bottom: l.struts.bottom.0,
            dim_inactive: l.dim_inactive,
            dim_factor: f64::from(l.dim_factor),
            layout_background_color: color_text(l.background_color),
            layout_fullscreen_backdrop_color: color_text(l.fullscreen_backdrop_color),
            rounding_power: l.rounding_power,
            tab_indicator_enabled: !l.tab_indicator.off,
            tab_indicator_hide_single: l.tab_indicator.hide_when_single_tab,
            tab_indicator_gap: l.tab_indicator.gap,
            tab_indicator_width: l.tab_indicator.width,
            tab_indicator_corner_radius: l.tab_indicator.corner_radius,
            tab_indicator_active_color: l
                .tab_indicator
                .active_color
                .map(color_text)
                .unwrap_or_default(),
            tab_indicator_inactive_color: l
                .tab_indicator
                .inactive_color
                .map(color_text)
                .unwrap_or_default(),
            tab_indicator_urgent_color: l
                .tab_indicator
                .urgent_color
                .map(color_text)
                .unwrap_or_default(),
            insert_hint_enabled: !l.insert_hint.off,
            insert_hint_color: color_text(l.insert_hint.color),
            overview_style: match config.overview.style {
                clarion_config::misc::OverviewStyle::Stage => "stage",
                clarion_config::misc::OverviewStyle::Strip => "strip",
            }
            .into(),
            overview_zoom: config.overview.zoom,
            overview_backdrop_color: color_text(config.overview.backdrop_color),
            stage_manager_enabled: config.overview.stage_manager.on,
            overview_shadow_enabled: !config.overview.workspace_shadow.off,
            overview_shadow_offset_x: config.overview.workspace_shadow.offset.x.0,
            overview_shadow_offset_y: config.overview.workspace_shadow.offset.y.0,
            overview_shadow_softness: config.overview.workspace_shadow.softness,
            overview_shadow_spread: config.overview.workspace_shadow.spread,
            overview_shadow_color: color_text(config.overview.workspace_shadow.color),
            preset_column_widths: preset_sizes_text(&config.layout.preset_column_widths),
            preset_window_heights: preset_sizes_text(&config.layout.preset_window_heights),
        }
    }
}

pub fn read(manager: &ConfigManager) -> Result<CommonSettings> {
    let config = manager.effective_config()?;
    read_with_sources(
        &config,
        manager.raw_content(),
        &manager.effective_source_contents()?,
    )
}

fn read_with_sources(
    config: &Config,
    source: &str,
    sources: &[(PathBuf, Option<Vec<u8>>)],
) -> Result<CommonSettings> {
    let mut settings = CommonSettings::from_config(config);
    if config.layout.border.active_gradient.is_none() {
        for candidate in std::iter::once(source.to_owned()).chain(
            sources
                .iter()
                .filter_map(|(_, bytes)| bytes.as_ref())
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned()),
        ) {
            let Some(properties) = gradient_memory(&candidate) else {
                continue;
            };
            let Ok(gradient) = parse_gradient(&properties) else {
                continue;
            };
            settings.border_gradient_from = color_text(gradient.from);
            settings.border_gradient_to = color_text(gradient.to);
            settings.border_gradient_angle = f64::from(gradient.angle);
            settings.border_gradient_relative = relative_text(gradient.relative_to).into();
            settings.border_gradient_in = interpolation_text(gradient.in_);
            settings.border_gradient_speed =
                gradient.rotate_deg_per_sec.map_or(0., |speed| speed.0);
            break;
        }
    }
    Ok(settings)
}

/// Produce a checked draft. Saving and IPC application remain the caller's job.
pub fn apply(
    manager: &mut ConfigManager,
    before: &CommonSettings,
    after: &CommonSettings,
) -> Result<()> {
    if before == after {
        return Ok(());
    }
    let sources = manager.effective_source_contents()?;
    let mut desired = manager.effective_config()?;
    let fresh = read_with_sources(&desired, manager.raw_content(), &sources)?;
    let mut content = manager.raw_content().to_owned();

    // Replaying a pending UI edit may coexist with changes to unrelated fields,
    // but it must not replace a newer edit of the same field.
    macro_rules! changed {
        ($field:ident) => {{
            let changed = before.$field != after.$field;
            anyhow::ensure!(
                !changed || before.$field == fresh.$field,
                "设置来源已改变（{}），请重新载入后再修改",
                stringify!($field)
            );
            changed
        }};
    }
    macro_rules! scalar {
        ($field:ident, $path:expr, $value:expr, $target:expr) => {
            if changed!($field) {
                content = set_scalar(&content, $path, &$value.to_string())?;
                $target = after.$field;
            }
        };
    }
    macro_rules! pair {
        ($field:ident, $path:expr, $target:expr) => {
            if changed!($field) {
                content = set_switch(&content, $path, after.$field)?;
                $target = !after.$field;
            }
        };
    }

    let replace_open_shader = changed!(open_custom_shader);
    let replace_close_shader = changed!(close_custom_shader);
    anyhow::ensure!(
        !replace_open_shader || !after.open_custom_shader,
        "不能通过内置效果选择创建自定义着色器"
    );
    anyhow::ensure!(
        !replace_close_shader || !after.close_custom_shader,
        "不能通过内置效果选择创建自定义着色器"
    );
    let open = changed!(window_open_enabled)
        || changed!(window_open_effect)
        || changed!(glide_enabled)
        || replace_open_shader;
    let close =
        changed!(window_close_enabled) || changed!(window_close_effect) || replace_close_shader;
    let movement = changed!(window_movement_enabled);
    let resize = changed!(window_resize_enabled);
    let workspace = changed!(workspace_switch_enabled) || changed!(cube_enabled);
    let wobble = changed!(wobble_enabled);
    for (modified, name) in [
        (open, "window-open"),
        (close, "window-close"),
        (movement, "window-movement"),
        (resize, "window-resize"),
        (workspace, "workspace-switch"),
        (wobble, "window-wobble"),
    ] {
        if modified {
            content = seed_group(&content, &sources, &["animations", name], &desired)?;
        }
    }
    for (modified, name) in [
        (
            changed!(touchpad_tap)
                || changed!(touchpad_natural_scroll)
                || changed!(touchpad_dwt)
                || changed!(touchpad_speed)
                || changed!(touchpad_scroll_factor),
            "touchpad",
        ),
        (
            changed!(mouse_natural_scroll) || changed!(mouse_speed),
            "mouse",
        ),
    ] {
        if modified {
            content = seed_group(&content, &sources, &["input", name], &desired)?;
        }
    }

    pair!(animations_enabled, &["animations"], desired.animations.off);
    scalar!(
        animation_speed,
        &["animations", "slowdown"],
        after.animation_speed,
        desired.animations.slowdown
    );
    for (modified, name, enabled) in [
        (
            changed!(window_open_enabled),
            "window-open",
            after.window_open_enabled,
        ),
        (
            changed!(window_close_enabled),
            "window-close",
            after.window_close_enabled,
        ),
        (movement, "window-movement", after.window_movement_enabled),
        (resize, "window-resize", after.window_resize_enabled),
        (
            changed!(workspace_switch_enabled),
            "workspace-switch",
            after.workspace_switch_enabled,
        ),
    ] {
        if modified {
            content = set_bare_flag(&content, &["animations", name, "off"], !enabled)?;
        }
    }
    if changed!(window_open_enabled) {
        desired.animations.window_open.anim.off = !after.window_open_enabled;
    }
    if changed!(window_close_enabled) {
        desired.animations.window_close.anim.off = !after.window_close_enabled;
    }
    if movement {
        desired.animations.window_movement.0.off = !after.window_movement_enabled;
    }
    if resize {
        desired.animations.window_resize.anim.off = !after.window_resize_enabled;
    }
    if changed!(workspace_switch_enabled) {
        desired.animations.workspace_switch.0.off = !after.workspace_switch_enabled;
    }
    if changed!(window_open_effect) || replace_open_shader {
        desired.animations.window_open.effect = after
            .window_open_effect
            .parse()
            .map_err(anyhow::Error::msg)?;
        content = set_scalar(
            &content,
            &["animations", "window-open", "effect"],
            &serde_json::to_string(&after.window_open_effect)?,
        )?;
        // A shader takes precedence over the built-in effect. Preserve it for
        // enable/disable; an explicit effect selection intentionally replaces it.
        content = remove(&content, &["animations", "window-open", "custom-shader"])?;
        desired.animations.window_open.custom_shader = None;
    }
    if changed!(window_close_effect) || replace_close_shader {
        // An effect-only close node also selects that effect's implicit timing.
        // Keep the currently effective duration/curve when this UI changes only
        // the visual effect, including when removing a previous custom shader.
        content = preserve_close_timing(&content, desired.animations.window_close.anim)?;
        desired.animations.window_close.effect = after
            .window_close_effect
            .parse()
            .map_err(anyhow::Error::msg)?;
        content = set_scalar(
            &content,
            &["animations", "window-close", "effect"],
            &serde_json::to_string(&after.window_close_effect)?,
        )?;
        content = remove(&content, &["animations", "window-close", "custom-shader"])?;
        desired.animations.window_close.custom_shader = None;
    }
    pair!(
        wobble_enabled,
        &["animations", "window-wobble"],
        desired.animations.window_wobble.off
    );
    pair!(
        cube_enabled,
        &["animations", "workspace-switch", "cube"],
        desired.animations.workspace_switch.1.off
    );
    if changed!(glide_enabled) {
        content = set_switch(
            &content,
            &["animations", "window-open", "glide"],
            after.glide_enabled,
        )?;
        desired.animations.window_open.glide.off = !after.glide_enabled;
    }

    scalar!(
        auto_stack,
        &["layout", "auto-stack-new-windows"],
        after.auto_stack,
        desired.layout.auto_stack_new_windows
    );
    scalar!(
        max_windows_per_column,
        &["layout", "max-windows-per-column"],
        after.max_windows_per_column,
        desired.layout.max_windows_per_column
    );
    scalar!(gaps, &["layout", "gaps"], after.gaps, desired.layout.gaps);
    scalar!(
        center_single,
        &["layout", "always-center-single-column"],
        after.center_single,
        desired.layout.always_center_single_column
    );
    scalar!(
        border_width,
        &["layout", "border", "width"],
        after.border_width,
        desired.layout.border.width
    );
    scalar!(
        focus_ring_width,
        &["layout", "focus-ring", "width"],
        after.focus_ring_width,
        desired.layout.focus_ring.width
    );
    scalar!(
        rounding,
        &["layout", "rounding"],
        after.rounding,
        desired.layout.rounding
    );
    pair!(
        border_enabled,
        &["layout", "border"],
        desired.layout.border.off
    );
    if changed!(border_gradient_enabled) {
        if after.border_gradient_enabled {
            content = create_gradient(&content, &sources, &mut desired, after)?;
        } else {
            let gradient = desired
                .layout
                .border
                .active_gradient
                .context("活动边框渐变来源无法确定，未修改原配置")?;
            let document = knuffel::parse_ast::<Span>("config.kdl", &content)
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            anyhow::ensure!(
                find_node(&document.nodes, GRADIENT_PATH)?.is_some(),
                "该渐变来自包含的配置文件，无法从这里关闭；请在源文件中删除 active-gradient"
            );
            let mut properties = None;
            for candidate in std::iter::once(content.clone()).chain(
                sources
                    .iter()
                    .filter_map(|(_, bytes)| bytes.as_ref())
                    .map(|bytes| String::from_utf8_lossy(bytes).into_owned()),
            ) {
                if let Some(block) = matching_gradient(&candidate, gradient)? {
                    properties = block
                        .strip_prefix("active-gradient")
                        .map(|rest| rest.trim().replace(['\n', '\r'], " "));
                    break;
                }
            }
            let properties =
                properties.unwrap_or(gradient_properties(gradient)?);
            content = set_gradient_memory(&content, &properties)?;
            content = remove(&content, GRADIENT_PATH)?;
            desired.layout.border.active_gradient = None;
        }
    }
    if after.border_gradient_enabled
        && (changed!(border_gradient_speed)
            || changed!(border_gradient_from)
            || changed!(border_gradient_to)
            || changed!(border_gradient_angle)
            || changed!(border_gradient_relative))
    {
        let speed = after.border_gradient_speed;
        anyhow::ensure!(
            speed.is_finite() && (-3600.0..=3600.0).contains(&speed),
            "渐变旋转速度必须是 -3600 到 3600 之间的值；0 表示静止渐变"
        );
        content = prepare_gradient(&content, &sources, &mut desired)?;
        content = set_gradient_rotation(&content, speed)?;
        if changed!(border_gradient_from) {
            let color = color_from_text(&after.border_gradient_from)?;
            content = set_gradient_property(
                &content,
                "from",
                &serde_json::to_string(&color_text(color))?,
            )?;
            desired
                .layout
                .border
                .active_gradient
                .as_mut()
                .context("请先启用活动边框渐变")?
                .from = color;
        }
        if changed!(border_gradient_to) {
            let color = color_from_text(&after.border_gradient_to)?;
            content = set_gradient_property(
                &content,
                "to",
                &serde_json::to_string(&color_text(color))?,
            )?;
            desired
                .layout
                .border
                .active_gradient
                .as_mut()
                .context("请先启用活动边框渐变")?
                .to = color;
        }
        if changed!(border_gradient_angle) {
            let angle = after.border_gradient_angle.round().clamp(-32768., 32767.) as i16;
            content = set_gradient_property(&content, "angle", &angle.to_string())?;
            desired
                .layout
                .border
                .active_gradient
                .as_mut()
                .context("请先启用活动边框渐变")?
                .angle = angle;
        }
        if changed!(border_gradient_relative) {
            let relative = match after.border_gradient_relative.as_str() {
                "window" => clarion_config::appearance::GradientRelativeTo::Window,
                "workspace-view" => {
                    clarion_config::appearance::GradientRelativeTo::WorkspaceView
                }
                _ => anyhow::bail!("渐变范围无效"),
            };
            content = set_gradient_property(
                &content,
                "relative-to",
                &serde_json::to_string(relative_text(relative))?,
            )?;
            desired
                .layout
                .border
                .active_gradient
                .as_mut()
                .context("请先启用活动边框渐变")?
                .relative_to = relative;
        }
        desired
            .layout
            .border
            .active_gradient
            .as_mut()
            .context("请先启用活动边框渐变")?
            .rotate_deg_per_sec = Some(clarion_config::FloatOrInt(speed));
    }
    pair!(
        focus_ring_enabled,
        &["layout", "focus-ring"],
        desired.layout.focus_ring.off
    );
    if changed!(shadow_enabled) {
        content = set_switch(&content, &["layout", "shadow"], after.shadow_enabled)?;
        desired.layout.shadow.on = after.shadow_enabled;
    }
    if changed!(center_column) {
        desired.layout.center_focused_column = match after.center_column.as_str() {
            "never" => clarion_config::layout::CenterFocusedColumn::Never,
            "always" => clarion_config::layout::CenterFocusedColumn::Always,
            "on-overflow" => clarion_config::layout::CenterFocusedColumn::OnOverflow,
            _ => anyhow::bail!("窗口居中选项无效"),
        };
        content = set_scalar(
            &content,
            &["layout", "center-focused-column"],
            &serde_json::to_string(&after.center_column)?,
        )?;
    }
    if changed!(default_width) {
        let node = match after.default_width {
            Some(PresetSize::Proportion(value)) => {
                anyhow::ensure!(value.is_finite() && value > 0., "默认窗口宽度必须大于零");
                format!("default-column-width {{ proportion {value}; }}")
            }
            Some(PresetSize::Fixed(value)) => {
                anyhow::ensure!(value > 0, "默认窗口宽度必须大于零");
                format!("default-column-width {{ fixed {value}; }}")
            }
            None => "default-column-width {}".into(),
        };
        ensure_unique(&content, &["layout", "default-column-width"])?;
        let mut editor = Editor::new(&content)?;
        editor.upsert(&["layout"], "default-column-width", &node)?;
        content = editor.finish();
        desired.layout.default_column_width = after.default_width;
    }
    pair!(blur_enabled, &["blur"], desired.blur.off);
    scalar!(
        blur_passes,
        &["blur", "passes"],
        after.blur_passes,
        desired.blur.passes
    );
    scalar!(
        blur_offset,
        &["blur", "offset"],
        after.blur_offset,
        desired.blur.offset
    );

    for (modified, name, value) in [
        (changed!(touchpad_tap), "tap", after.touchpad_tap),
        (
            changed!(touchpad_natural_scroll),
            "natural-scroll",
            after.touchpad_natural_scroll,
        ),
        (changed!(touchpad_dwt), "dwt", after.touchpad_dwt),
    ] {
        if modified {
            content = set_bare_flag(&content, &["input", "touchpad", name], value)?;
        }
    }
    if changed!(touchpad_tap) {
        desired.input.touchpad.tap = after.touchpad_tap;
    }
    if changed!(touchpad_natural_scroll) {
        desired.input.touchpad.natural_scroll = after.touchpad_natural_scroll;
    }
    if changed!(touchpad_dwt) {
        desired.input.touchpad.dwt = after.touchpad_dwt;
    }
    scalar!(
        touchpad_speed,
        &["input", "touchpad", "accel-speed"],
        after.touchpad_speed,
        desired.input.touchpad.accel_speed.0
    );
    scalar!(
        mouse_speed,
        &["input", "mouse", "accel-speed"],
        after.mouse_speed,
        desired.input.mouse.accel_speed.0
    );
    if changed!(mouse_natural_scroll) {
        content = set_bare_flag(
            &content,
            &["input", "mouse", "natural-scroll"],
            after.mouse_natural_scroll,
        )?;
        desired.input.mouse.natural_scroll = after.mouse_natural_scroll;
    }
    for (modified, name, value) in [
        (changed!(touchpad_drag_lock), "drag-lock", after.touchpad_drag_lock),
        (changed!(touchpad_left_handed), "left-handed", after.touchpad_left_handed),
        (
            changed!(touchpad_middle_emulation),
            "middle-emulation",
            after.touchpad_middle_emulation,
        ),
        (
            changed!(touchpad_disable_on_external_mouse),
            "disabled-on-external-mouse",
            after.touchpad_disable_on_external_mouse,
        ),
        (changed!(touchpad_dwtp), "dwtp", after.touchpad_dwtp),
    ] {
        if modified {
            content = set_bare_flag(&content, &["input", "touchpad", name], value)?;
        }
    }
    for (modified, name, value) in [
        (changed!(mouse_left_handed), "left-handed", after.mouse_left_handed),
        (
            changed!(mouse_middle_emulation),
            "middle-emulation",
            after.mouse_middle_emulation,
        ),
    ] {
        if modified {
            content = set_bare_flag(&content, &["input", "mouse", name], value)?;
        }
    }
    if changed!(touchpad_drag_lock) {
        desired.input.touchpad.drag_lock = after.touchpad_drag_lock;
    }
    if changed!(touchpad_left_handed) {
        desired.input.touchpad.left_handed = after.touchpad_left_handed;
    }
    if changed!(touchpad_middle_emulation) {
        desired.input.touchpad.middle_emulation = after.touchpad_middle_emulation;
    }
    if changed!(touchpad_disable_on_external_mouse) {
        desired.input.touchpad.disabled_on_external_mouse = after.touchpad_disable_on_external_mouse;
    }
    if changed!(touchpad_dwtp) {
        desired.input.touchpad.dwtp = after.touchpad_dwtp;
    }
    if changed!(mouse_left_handed) {
        desired.input.mouse.left_handed = after.mouse_left_handed;
    }
    if changed!(mouse_middle_emulation) {
        desired.input.mouse.middle_emulation = after.mouse_middle_emulation;
    }
    if changed!(touchpad_drag) {
        content = set_scalar(
            &content,
            &["input", "touchpad", "drag"],
            if after.touchpad_drag { "true" } else { "false" },
        )?;
        desired.input.touchpad.drag = Some(after.touchpad_drag);
    }
    if changed!(touchpad_click_method) {
        content = set_optional_input_scalar(
            &content,
            &["input", "touchpad", "click-method"],
            &after.touchpad_click_method,
        )?;
        desired.input.touchpad.click_method = click_method_value(&after.touchpad_click_method);
    }
    if changed!(touchpad_accel_profile) {
        content = set_optional_input_scalar(
            &content,
            &["input", "touchpad", "accel-profile"],
            &after.touchpad_accel_profile,
        )?;
        desired.input.touchpad.accel_profile =
            accel_profile_value(&after.touchpad_accel_profile);
    }
    if changed!(touchpad_scroll_method) {
        content = set_optional_input_scalar(
            &content,
            &["input", "touchpad", "scroll-method"],
            &after.touchpad_scroll_method,
        )?;
        desired.input.touchpad.scroll_method =
            scroll_method_value(&after.touchpad_scroll_method);
    }
    if changed!(touchpad_tap_button_map) {
        content = set_optional_input_scalar(
            &content,
            &["input", "touchpad", "tap-button-map"],
            &after.touchpad_tap_button_map,
        )?;
        desired.input.touchpad.tap_button_map =
            tap_button_map_value(&after.touchpad_tap_button_map);
    }
    if changed!(mouse_accel_profile) {
        content = set_optional_input_scalar(
            &content,
            &["input", "mouse", "accel-profile"],
            &after.mouse_accel_profile,
        )?;
        desired.input.mouse.accel_profile = accel_profile_value(&after.mouse_accel_profile);
    }
    if changed!(mouse_scroll_method) {
        content = set_optional_input_scalar(
            &content,
            &["input", "mouse", "scroll-method"],
            &after.mouse_scroll_method,
        )?;
        desired.input.mouse.scroll_method = scroll_method_value(&after.mouse_scroll_method);
    }
    if changed!(trackpoint_enabled) {
        content = set_bare_flag(
            &content,
            &["input", "trackpoint", "off"],
            !after.trackpoint_enabled,
        )?;
        desired.input.trackpoint.off = !after.trackpoint_enabled;
    }
    if changed!(trackpoint_natural_scroll) {
        content = set_bare_flag(
            &content,
            &["input", "trackpoint", "natural-scroll"],
            after.trackpoint_natural_scroll,
        )?;
        desired.input.trackpoint.natural_scroll = after.trackpoint_natural_scroll;
    }
    scalar!(
        trackpoint_speed,
        &["input", "trackpoint", "accel-speed"],
        after.trackpoint_speed,
        desired.input.trackpoint.accel_speed.0
    );
    if changed!(trackpoint_accel_profile) {
        content = set_optional_input_scalar(
            &content,
            &["input", "trackpoint", "accel-profile"],
            &after.trackpoint_accel_profile,
        )?;
        desired.input.trackpoint.accel_profile =
            accel_profile_value(&after.trackpoint_accel_profile);
    }
    if changed!(trackpoint_left_handed) {
        content = set_bare_flag(
            &content,
            &["input", "trackpoint", "left-handed"],
            after.trackpoint_left_handed,
        )?;
        desired.input.trackpoint.left_handed = after.trackpoint_left_handed;
    }
    if changed!(trackpoint_middle_emulation) {
        content = set_bare_flag(
            &content,
            &["input", "trackpoint", "middle-emulation"],
            after.trackpoint_middle_emulation,
        )?;
        desired.input.trackpoint.middle_emulation = after.trackpoint_middle_emulation;
    }
    if changed!(screenshot_path) {
        let text = after.screenshot_path.trim();
        if text.is_empty() {
            content = remove(&content, &["screenshot-path"])?;
            desired.screenshot_path = clarion_config::ScreenshotPath::default();
        } else {
            content = set_scalar(
                &content,
                &["screenshot-path"],
                &serde_json::to_string(text)?,
            )?;
            desired.screenshot_path.0 = Some(text.to_owned());
        }
    }
    if changed!(xwayland_enabled) {
        content = set_switch(&content, &["xwayland-satellite"], after.xwayland_enabled)?;
        desired.xwayland_satellite.off = !after.xwayland_enabled;
    }
    if changed!(xwayland_path) {
        content = set_scalar(
            &content,
            &["xwayland-satellite", "path"],
            &serde_json::to_string(after.xwayland_path.trim())?,
        )?;
        desired.xwayland_satellite.path = after.xwayland_path.trim().to_owned();
    }
    if changed!(prefer_no_csd) {
        content = set_bare_flag(&content, &["prefer-no-csd"], after.prefer_no_csd)?;
        desired.prefer_no_csd = after.prefer_no_csd;
    }
    if changed!(clipboard_disable_primary) {
        content = set_bare_flag(
            &content,
            &["clipboard", "disable-primary"],
            after.clipboard_disable_primary,
        )?;
        desired.clipboard.disable_primary = after.clipboard_disable_primary;
    }
    for (modified, name, value) in [
        (
            changed!(hotkey_skip_at_startup),
            "skip-at-startup",
            after.hotkey_skip_at_startup,
        ),
        (
            changed!(hotkey_hide_not_bound),
            "hide-not-bound",
            after.hotkey_hide_not_bound,
        ),
    ] {
        if modified {
            content = set_bare_flag(&content, &["hotkey-overlay", name], value)?;
        }
    }
    if changed!(hotkey_skip_at_startup) {
        desired.hotkey_overlay.skip_at_startup = after.hotkey_skip_at_startup;
    }
    if changed!(hotkey_hide_not_bound) {
        desired.hotkey_overlay.hide_not_bound = after.hotkey_hide_not_bound;
    }
    if changed!(config_notification_disable_failed) {
        content = set_bare_flag(
            &content,
            &["config-notification", "disable-failed"],
            after.config_notification_disable_failed,
        )?;
        desired.config_notification.disable_failed = after.config_notification_disable_failed;
    }
    if changed!(touchpad_scroll_factor) {
        // Keep axis-specific overrides; this control changes the common base.
        content = set_scalar(
            &content,
            &["input", "touchpad", "scroll-factor"],
            &after.touchpad_scroll_factor.to_string(),
        )?;
        desired
            .input
            .touchpad
            .scroll_factor
            .get_or_insert_default()
            .base = Some(clarion_config::FloatOrInt(after.touchpad_scroll_factor));
    }
    scalar!(
        keyboard_numlock,
        &["input", "keyboard", "numlock"],
        after.keyboard_numlock,
        desired.input.keyboard.numlock
    );
    scalar!(
        keyboard_repeat_rate,
        &["input", "keyboard", "repeat-rate"],
        after.keyboard_repeat_rate,
        desired.input.keyboard.repeat_rate
    );
    scalar!(
        keyboard_repeat_delay,
        &["input", "keyboard", "repeat-delay"],
        after.keyboard_repeat_delay,
        desired.input.keyboard.repeat_delay
    );

    // Idle policy, cursor, XKB, hot corners and the remaining blur knobs are
    // flat options. Their blocks may be absent or inherited from an include;
    // set_scalar/set_bare_flag upsert only the changed node so comments and
    // unrelated values stay byte-identical.
    if changed!(idle_enabled) {
        content = set_bare_flag(&content, &["idle", "off"], !after.idle_enabled)?;
        desired.idle.off = !after.idle_enabled;
    }
    scalar!(idle_timeout, &["idle", "timeout"], after.idle_timeout, desired.idle.timeout);
    scalar!(
        idle_blank_timeout,
        &["idle", "blank-timeout"],
        after.idle_blank_timeout,
        desired.idle.blank_timeout
    );
    if changed!(idle_action) {
        desired.idle.action = match after.idle_action.as_str() {
            "none" => IdleAction::None,
            "lock" => IdleAction::Lock,
            "suspend" => IdleAction::Suspend,
            _ => anyhow::bail!("空闲动作无效"),
        };
        content = set_scalar(
            &content,
            &["idle", "action"],
            &serde_json::to_string(&after.idle_action)?,
        )?;
    }
    if changed!(idle_lock_command) {
        let command = after.idle_lock_command.trim();
        if command.is_empty() {
            content = remove(&content, &["idle", "lock-command"])?;
            desired.idle.lock_command = None;
        } else {
            content = set_scalar(
                &content,
                &["idle", "lock-command"],
                &serde_json::to_string(command)?,
            )?;
            desired.idle.lock_command = Some(command.to_owned());
        }
    }
    if changed!(idle_lock_before_suspend) {
        content = set_bare_flag(
            &content,
            &["idle", "lock-before-suspend"],
            after.idle_lock_before_suspend,
        )?;
        desired.idle.lock_before_suspend = after.idle_lock_before_suspend;
    }
    if changed!(cursor_theme) {
        let theme = after.cursor_theme.trim();
        if theme.is_empty() {
            content = remove(&content, &["cursor", "xcursor-theme"])?;
            desired.cursor.xcursor_theme = "default".into();
        } else {
            content = set_scalar(
                &content,
                &["cursor", "xcursor-theme"],
                &serde_json::to_string(theme)?,
            )?;
            desired.cursor.xcursor_theme = theme.to_owned();
        }
    }
    scalar!(
        cursor_size,
        &["cursor", "xcursor-size"],
        after.cursor_size,
        desired.cursor.xcursor_size
    );
    if changed!(cursor_hide_when_typing) {
        content = set_bare_flag(
            &content,
            &["cursor", "hide-when-typing"],
            after.cursor_hide_when_typing,
        )?;
        desired.cursor.hide_when_typing = after.cursor_hide_when_typing;
    }
    if changed!(cursor_hide_after_ms) {
        match after.cursor_hide_after_ms {
            Some(milliseconds) => {
                content = set_scalar(
                    &content,
                    &["cursor", "hide-after-inactive-ms"],
                    &milliseconds.to_string(),
                )?;
                desired.cursor.hide_after_inactive_ms = Some(milliseconds);
            }
            None => {
                content = remove(&content, &["cursor", "hide-after-inactive-ms"])?;
                desired.cursor.hide_after_inactive_ms = None;
            }
        }
    }
    for (modified, name, value, target) in [
        (
            changed!(keyboard_xkb_layout),
            "layout",
            after.keyboard_xkb_layout.trim().to_owned(),
            &mut desired.input.keyboard.xkb.layout,
        ),
        (
            changed!(keyboard_xkb_variant),
            "variant",
            after.keyboard_xkb_variant.trim().to_owned(),
            &mut desired.input.keyboard.xkb.variant,
        ),
    ] {
        if modified {
            if value.is_empty() {
                content = remove(&content, &["input", "keyboard", "xkb", name])?;
            } else {
                content = set_scalar(
                    &content,
                    &["input", "keyboard", "xkb", name],
                    &serde_json::to_string(&value)?,
                )?;
            }
            *target = value;
        }
    }
    if changed!(keyboard_xkb_options) {
        let options = after.keyboard_xkb_options.trim();
        if options.is_empty() {
            content = remove(&content, &["input", "keyboard", "xkb", "options"])?;
            desired.input.keyboard.xkb.options = None;
        } else {
            content = set_scalar(
                &content,
                &["input", "keyboard", "xkb", "options"],
                &serde_json::to_string(options)?,
            )?;
            desired.input.keyboard.xkb.options = Some(options.to_owned());
        }
    }
    if changed!(hot_corners_enabled) {
        content = set_bare_flag(
            &content,
            &["gestures", "hot-corners", "off"],
            !after.hot_corners_enabled,
        )?;
        desired.gestures.hot_corners.off = !after.hot_corners_enabled;
    }
    for (modified, name, value) in [
        (
            changed!(hot_corner_top_left),
            "top-left",
            after.hot_corner_top_left,
        ),
        (
            changed!(hot_corner_top_right),
            "top-right",
            after.hot_corner_top_right,
        ),
        (
            changed!(hot_corner_bottom_left),
            "bottom-left",
            after.hot_corner_bottom_left,
        ),
        (
            changed!(hot_corner_bottom_right),
            "bottom-right",
            after.hot_corner_bottom_right,
        ),
    ] {
        if modified {
            content =
                set_bare_flag(&content, &["gestures", "hot-corners", name], value)?;
        }
    }
    if changed!(hot_corner_top_left) {
        desired.gestures.hot_corners.top_left = after.hot_corner_top_left;
    }
    if changed!(hot_corner_top_right) {
        desired.gestures.hot_corners.top_right = after.hot_corner_top_right;
    }
    if changed!(hot_corner_bottom_left) {
        desired.gestures.hot_corners.bottom_left = after.hot_corner_bottom_left;
    }
    if changed!(hot_corner_bottom_right) {
        desired.gestures.hot_corners.bottom_right = after.hot_corner_bottom_right;
    }
    for (modified, name, value, target) in [
        (
            changed!(hot_corner_top_left_action),
            "top-left-action",
            after.hot_corner_top_left_action.as_str(),
            &mut desired.gestures.hot_corners.top_left_action,
        ),
        (
            changed!(hot_corner_top_right_action),
            "top-right-action",
            after.hot_corner_top_right_action.as_str(),
            &mut desired.gestures.hot_corners.top_right_action,
        ),
        (
            changed!(hot_corner_bottom_left_action),
            "bottom-left-action",
            after.hot_corner_bottom_left_action.as_str(),
            &mut desired.gestures.hot_corners.bottom_left_action,
        ),
        (
            changed!(hot_corner_bottom_right_action),
            "bottom-right-action",
            after.hot_corner_bottom_right_action.as_str(),
            &mut desired.gestures.hot_corners.bottom_right_action,
        ),
    ] {
        if modified {
            let action = parse_hot_corner_action(value)?;
            content = set_scalar(
                &content,
                &["gestures", "hot-corners", name],
                &serde_json::to_string(value)?,
            )?;
            *target = Some(action);
        }
    }
    scalar!(
        blur_noise,
        &["blur", "noise"],
        after.blur_noise,
        desired.blur.noise
    );
    scalar!(
        blur_saturation,
        &["blur", "saturation"],
        after.blur_saturation,
        desired.blur.saturation
    );

    for (modified, path, value, target) in [
        (
            changed!(border_active_color),
            &["layout", "border", "active-color"][..],
            after.border_active_color.as_str(),
            &mut desired.layout.border.active_color,
        ),
        (
            changed!(border_inactive_color),
            &["layout", "border", "inactive-color"][..],
            after.border_inactive_color.as_str(),
            &mut desired.layout.border.inactive_color,
        ),
        (
            changed!(border_urgent_color),
            &["layout", "border", "urgent-color"][..],
            after.border_urgent_color.as_str(),
            &mut desired.layout.border.urgent_color,
        ),
        (
            changed!(focus_ring_active_color),
            &["layout", "focus-ring", "active-color"][..],
            after.focus_ring_active_color.as_str(),
            &mut desired.layout.focus_ring.active_color,
        ),
        (
            changed!(focus_ring_inactive_color),
            &["layout", "focus-ring", "inactive-color"][..],
            after.focus_ring_inactive_color.as_str(),
            &mut desired.layout.focus_ring.inactive_color,
        ),
        (
            changed!(focus_ring_urgent_color),
            &["layout", "focus-ring", "urgent-color"][..],
            after.focus_ring_urgent_color.as_str(),
            &mut desired.layout.focus_ring.urgent_color,
        ),
        (
            changed!(shadow_color),
            &["layout", "shadow", "color"][..],
            after.shadow_color.as_str(),
            &mut desired.layout.shadow.color,
        ),
        (
            changed!(layout_background_color),
            &["layout", "background-color"][..],
            after.layout_background_color.as_str(),
            &mut desired.layout.background_color,
        ),
        (
            changed!(layout_fullscreen_backdrop_color),
            &["layout", "fullscreen-backdrop-color"][..],
            after.layout_fullscreen_backdrop_color.as_str(),
            &mut desired.layout.fullscreen_backdrop_color,
        ),
        (
            changed!(insert_hint_color),
            &["layout", "insert-hint", "color"][..],
            after.insert_hint_color.as_str(),
            &mut desired.layout.insert_hint.color,
        ),
        (
            changed!(overview_backdrop_color),
            &["overview", "backdrop-color"][..],
            after.overview_backdrop_color.as_str(),
            &mut desired.overview.backdrop_color,
        ),
    ] {
        if modified {
            let color = color_from_text(value)?;
            content = set_scalar(
                &content,
                path,
                &serde_json::to_string(&color_text(color))?,
            )?;
            *target = color;
        }
    }
    if changed!(shadow_inactive_color) {
        if after.shadow_inactive_color.trim().is_empty() {
            content = remove(&content, &["layout", "shadow", "inactive-color"])?;
            desired.layout.shadow.inactive_color = None;
        } else {
            let color = color_from_text(&after.shadow_inactive_color)?;
            content = set_scalar(
                &content,
                &["layout", "shadow", "inactive-color"],
                &serde_json::to_string(&color_text(color))?,
            )?;
            desired.layout.shadow.inactive_color = Some(color);
        }
    }
    if changed!(shadow_offset_x) || changed!(shadow_offset_y) {
        content = set_shadow_offset(
            &content,
            after.shadow_offset_x,
            after.shadow_offset_y,
        )?;
        desired.layout.shadow.offset.x = clarion_config::FloatOrInt(after.shadow_offset_x);
        desired.layout.shadow.offset.y = clarion_config::FloatOrInt(after.shadow_offset_y);
    }
    scalar!(
        shadow_softness,
        &["layout", "shadow", "softness"],
        after.shadow_softness,
        desired.layout.shadow.softness
    );
    scalar!(
        shadow_spread,
        &["layout", "shadow", "spread"],
        after.shadow_spread,
        desired.layout.shadow.spread
    );
    if changed!(shadow_draw_behind) {
        content = set_scalar(
            &content,
            &["layout", "shadow", "draw-behind-window"],
            if after.shadow_draw_behind { "true" } else { "false" },
        )?;
        desired.layout.shadow.draw_behind_window = after.shadow_draw_behind;
    }
    if changed!(struts_left)
        || changed!(struts_right)
        || changed!(struts_top)
        || changed!(struts_bottom)
    {
        content = set_struts(
            &content,
            after.struts_left,
            after.struts_right,
            after.struts_top,
            after.struts_bottom,
        )?;
        desired.layout.struts.left = clarion_config::FloatOrInt(after.struts_left);
        desired.layout.struts.right = clarion_config::FloatOrInt(after.struts_right);
        desired.layout.struts.top = clarion_config::FloatOrInt(after.struts_top);
        desired.layout.struts.bottom = clarion_config::FloatOrInt(after.struts_bottom);
    }
    if changed!(dim_inactive) {
        content = set_bare_flag(
            &content,
            &["layout", "dim-inactive"],
            after.dim_inactive,
        )?;
        desired.layout.dim_inactive = after.dim_inactive;
    }
    if changed!(dim_factor) {
        content = set_scalar(
            &content,
            &["layout", "dim-factor"],
            &after.dim_factor.to_string(),
        )?;
        desired.layout.dim_factor = after.dim_factor as f32;
    }
    scalar!(
        rounding_power,
        &["layout", "rounding-power"],
        after.rounding_power,
        desired.layout.rounding_power
    );
    if changed!(tab_indicator_enabled) {
        content = set_bare_flag(
            &content,
            &["layout", "tab-indicator", "off"],
            !after.tab_indicator_enabled,
        )?;
        desired.layout.tab_indicator.off = !after.tab_indicator_enabled;
    }
    if changed!(tab_indicator_hide_single) {
        content = set_bare_flag(
            &content,
            &["layout", "tab-indicator", "hide-when-single-tab"],
            after.tab_indicator_hide_single,
        )?;
        desired.layout.tab_indicator.hide_when_single_tab = after.tab_indicator_hide_single;
    }
    scalar!(
        tab_indicator_gap,
        &["layout", "tab-indicator", "gap"],
        after.tab_indicator_gap,
        desired.layout.tab_indicator.gap
    );
    scalar!(
        tab_indicator_width,
        &["layout", "tab-indicator", "width"],
        after.tab_indicator_width,
        desired.layout.tab_indicator.width
    );
    scalar!(
        tab_indicator_corner_radius,
        &["layout", "tab-indicator", "corner-radius"],
        after.tab_indicator_corner_radius,
        desired.layout.tab_indicator.corner_radius
    );
    for (modified, name, value, target) in [
        (
            changed!(tab_indicator_active_color),
            "active-color",
            after.tab_indicator_active_color.as_str(),
            &mut desired.layout.tab_indicator.active_color,
        ),
        (
            changed!(tab_indicator_inactive_color),
            "inactive-color",
            after.tab_indicator_inactive_color.as_str(),
            &mut desired.layout.tab_indicator.inactive_color,
        ),
        (
            changed!(tab_indicator_urgent_color),
            "urgent-color",
            after.tab_indicator_urgent_color.as_str(),
            &mut desired.layout.tab_indicator.urgent_color,
        ),
    ] {
        if modified {
            if value.trim().is_empty() {
                content = remove(&content, &["layout", "tab-indicator", name])?;
                *target = None;
            } else {
                let color = color_from_text(value)?;
                content = set_scalar(
                    &content,
                    &["layout", "tab-indicator", name],
                    &serde_json::to_string(&color_text(color))?,
                )?;
                *target = Some(color);
            }
        }
    }
    if changed!(insert_hint_enabled) {
        content = set_bare_flag(
            &content,
            &["layout", "insert-hint", "off"],
            !after.insert_hint_enabled,
        )?;
        desired.layout.insert_hint.off = !after.insert_hint_enabled;
    }
    if changed!(overview_style) {
        let style = match after.overview_style.as_str() {
            "strip" => clarion_config::misc::OverviewStyle::Strip,
            "stage" => clarion_config::misc::OverviewStyle::Stage,
            _ => anyhow::bail!("概览风格无效"),
        };
        content = set_scalar(
            &content,
            &["overview", "style"],
            &serde_json::to_string(&after.overview_style)?,
        )?;
        desired.overview.style = style;
    }
    scalar!(
        overview_zoom,
        &["overview", "zoom"],
        after.overview_zoom,
        desired.overview.zoom
    );
    if changed!(stage_manager_enabled) {
        content = set_switch(
            &content,
            &["overview", "stage-manager"],
            after.stage_manager_enabled,
        )?;
        desired.overview.stage_manager.on = after.stage_manager_enabled;
    }
    if changed!(overview_shadow_enabled) {
        content = set_switch(
            &content,
            &["overview", "workspace-shadow"],
            after.overview_shadow_enabled,
        )?;
        desired.overview.workspace_shadow.off = !after.overview_shadow_enabled;
    }
    if changed!(overview_shadow_offset_x) || changed!(overview_shadow_offset_y) {
        let mut editor = Editor::new(&content)?;
        editor.upsert(
            &["overview", "workspace-shadow"],
            "offset",
            &format!(
                "offset x={} y={}",
                after.overview_shadow_offset_x, after.overview_shadow_offset_y
            ),
        )?;
        content = editor.finish();
        desired.overview.workspace_shadow.offset.x =
            clarion_config::FloatOrInt(after.overview_shadow_offset_x);
        desired.overview.workspace_shadow.offset.y =
            clarion_config::FloatOrInt(after.overview_shadow_offset_y);
    }
    scalar!(
        overview_shadow_softness,
        &["overview", "workspace-shadow", "softness"],
        after.overview_shadow_softness,
        desired.overview.workspace_shadow.softness
    );
    scalar!(
        overview_shadow_spread,
        &["overview", "workspace-shadow", "spread"],
        after.overview_shadow_spread,
        desired.overview.workspace_shadow.spread
    );
    if changed!(overview_shadow_color) {
        let color = color_from_text(&after.overview_shadow_color)?;
        content = set_scalar(
            &content,
            &["overview", "workspace-shadow", "color"],
            &serde_json::to_string(&color_text(color))?,
        )?;
        desired.overview.workspace_shadow.color = color;
    }
    if changed!(preset_column_widths) {
        let sizes = parse_preset_sizes(&after.preset_column_widths)?;
        if sizes.is_empty() {
            content = remove(&content, &["layout", "preset-column-widths"])?;
        } else {
            let mut editor = Editor::new(&content)?;
            editor.upsert(
                &["layout"],
                "preset-column-widths",
                &preset_sizes_block("preset-column-widths", &sizes),
            )?;
            content = editor.finish();
        }
        desired.layout.preset_column_widths = sizes;
    }
    if changed!(preset_window_heights) {
        let sizes = parse_preset_sizes(&after.preset_window_heights)?;
        if sizes.is_empty() {
            content = remove(&content, &["layout", "preset-window-heights"])?;
        } else {
            let mut editor = Editor::new(&content)?;
            editor.upsert(
                &["layout"],
                "preset-window-heights",
                &preset_sizes_block("preset-window-heights", &sizes),
            )?;
            content = editor.finish();
        }
        desired.layout.preset_window_heights = sizes;
    }

    // At the root, Clarion's legacy decoder implicitly enables a newly added
    // `border { ... }` block. Merely changing its width/gradient must preserve
    // an inherited or default disabled border, rather than turn it on.
    if desired.layout.border.off
        && !Editor::new(manager.raw_content())?.exists(&["layout", "border"])
        && Editor::new(&content)?.exists(&["layout", "border"])
    {
        content = set_bare_flag(&content, &["layout", "border", "off"], true)?;
    }
    let candidate = manager.merged_config(&content)?;
    anyhow::ensure!(
        candidate == desired,
        "设置被后续包含文件覆盖或修改会重置其他参数，原草稿已保留；请检查配置来源与载入顺序"
    );
    anyhow::ensure!(
        manager.effective_source_contents()? == sources,
        "包含文件在修改期间发生变化，原草稿已保留，请重新载入"
    );
    manager.set_raw_content(content);
    Ok(())
}

fn set_optional_input_scalar(source: &str, path: &[&str], value: &str) -> Result<String> {
    if value.is_empty() {
        remove(source, path)
    } else {
        set_scalar(source, path, &serde_json::to_string(value)?)
    }
}

fn click_method_value(text: &str) -> Option<clarion_config::input::ClickMethod> {
    match text {
        "clickfinger" => Some(clarion_config::input::ClickMethod::Clickfinger),
        "button-areas" => Some(clarion_config::input::ClickMethod::ButtonAreas),
        _ => None,
    }
}

fn accel_profile_value(text: &str) -> Option<clarion_config::input::AccelProfile> {
    match text {
        "adaptive" => Some(clarion_config::input::AccelProfile::Adaptive),
        "flat" => Some(clarion_config::input::AccelProfile::Flat),
        _ => None,
    }
}

fn scroll_method_value(text: &str) -> Option<clarion_config::input::ScrollMethod> {
    match text {
        "no-scroll" => Some(clarion_config::input::ScrollMethod::NoScroll),
        "two-finger" => Some(clarion_config::input::ScrollMethod::TwoFinger),
        "edge" => Some(clarion_config::input::ScrollMethod::Edge),
        "on-button-down" => Some(clarion_config::input::ScrollMethod::OnButtonDown),
        _ => None,
    }
}

fn tap_button_map_value(text: &str) -> Option<clarion_config::input::TapButtonMap> {
    match text {
        "left-right-middle" => Some(clarion_config::input::TapButtonMap::LeftRightMiddle),
        "left-middle-right" => Some(clarion_config::input::TapButtonMap::LeftMiddleRight),
        _ => None,
    }
}

fn click_method_text(value: Option<clarion_config::input::ClickMethod>) -> String {
    match value {
        Some(clarion_config::input::ClickMethod::Clickfinger) => "clickfinger",
        Some(clarion_config::input::ClickMethod::ButtonAreas) => "button-areas",
        None => "",
    }
    .into()
}

fn accel_profile_text(value: Option<clarion_config::input::AccelProfile>) -> String {
    match value {
        Some(clarion_config::input::AccelProfile::Adaptive) => "adaptive",
        Some(clarion_config::input::AccelProfile::Flat) => "flat",
        None => "",
    }
    .into()
}

fn scroll_method_text(value: Option<clarion_config::input::ScrollMethod>) -> String {
    match value {
        Some(clarion_config::input::ScrollMethod::NoScroll) => "no-scroll",
        Some(clarion_config::input::ScrollMethod::TwoFinger) => "two-finger",
        Some(clarion_config::input::ScrollMethod::Edge) => "edge",
        Some(clarion_config::input::ScrollMethod::OnButtonDown) => "on-button-down",
        None => "",
    }
    .into()
}

fn tap_button_map_text(value: Option<clarion_config::input::TapButtonMap>) -> String {
    match value {
        Some(clarion_config::input::TapButtonMap::LeftRightMiddle) => "left-right-middle",
        Some(clarion_config::input::TapButtonMap::LeftMiddleRight) => "left-middle-right",
        None => "",
    }
    .into()
}

pub fn preset_sizes_text(sizes: &[PresetSize]) -> String {
    sizes
        .iter()
        .map(|size| match size {
            PresetSize::Proportion(value) => format!("{}%", value * 100.),
            PresetSize::Fixed(value) => format!("{value}px"),
        })
        .collect::<Vec<_>>()
        .join(",")
}

pub fn parse_preset_sizes(text: &str) -> Result<Vec<PresetSize>> {
    let mut sizes = Vec::new();
    for token in text.split(',').map(str::trim).filter(|token| !token.is_empty()) {
        if let Some(value) = token.strip_suffix('%') {
            let value = value.trim().parse::<f64>()?;
            anyhow::ensure!(value.is_finite() && value > 0., "预设比例必须大于零");
            sizes.push(PresetSize::Proportion(value / 100.));
        } else if let Some(value) = token.strip_suffix("px") {
            let value = value.trim().parse::<i32>()?;
            anyhow::ensure!(value > 0, "预设像素宽度必须大于零");
            sizes.push(PresetSize::Fixed(value));
        } else {
            anyhow::bail!("预设尺寸请写成 25% 或 960px");
        }
    }
    anyhow::ensure!(sizes.len() <= 16, "预设尺寸最多 16 项");
    Ok(sizes)
}

fn preset_sizes_block(name: &str, sizes: &[PresetSize]) -> String {
    let mut block = format!("{name} {{\n");
    for size in sizes {
        match size {
            PresetSize::Proportion(value) => {
                block.push_str(&format!("        proportion {value}\n"));
            }
            PresetSize::Fixed(value) => {
                block.push_str(&format!("        fixed {value}\n"));
            }
        }
    }
    block.push_str("    }");
    block
}

fn color_text(color: clarion_config::Color) -> String {
    let [r, g, b, a] = color.to_array_unpremul();
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    if a >= 0.999 {
        format!("#{:02x}{:02x}{:02x}", channel(r), channel(g), channel(b))
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            channel(r),
            channel(g),
            channel(b),
            channel(a)
        )
    }
}

fn color_from_text(text: &str) -> Result<clarion_config::Color> {
    clarion_config::Color::from_str(text.trim())
        .map_err(|error| anyhow::anyhow!("颜色格式无效：{error}"))
}

fn set_shadow_offset(source: &str, x: f64, y: f64) -> Result<String> {
    let mut editor = Editor::new(source)?;
    editor.upsert(
        &["layout", "shadow"],
        "offset",
        &format!("offset x={x} y={y}"),
    )?;
    Ok(editor.finish())
}

fn set_struts(source: &str, left: f64, right: f64, top: f64, bottom: f64) -> Result<String> {
    let mut content = source.to_owned();
    for (name, value) in [
        ("left", left),
        ("right", right),
        ("top", top),
        ("bottom", bottom),
    ] {
        content = set_scalar(&content, &["layout", "struts", name], &value.to_string())?;
    }
    Ok(content)
}

fn hot_corner_action_key(action: clarion_config::gestures::HotCornerAction) -> String {
    use clarion_config::gestures::HotCornerAction;
    match action {
        HotCornerAction::None => "none",
        HotCornerAction::Overview => "overview",
        HotCornerAction::Expo => "expo",
        HotCornerAction::WindowSpread => "window-spread",
        HotCornerAction::Launcher => "launcher",
        HotCornerAction::ControlCenter => "control-center",
        HotCornerAction::Settings => "settings",
    }
    .into()
}

fn parse_hot_corner_action(
    value: &str,
) -> Result<clarion_config::gestures::HotCornerAction> {
    use clarion_config::gestures::HotCornerAction;
    Ok(match value {
        "none" => HotCornerAction::None,
        "overview" => HotCornerAction::Overview,
        "expo" => HotCornerAction::Expo,
        "window-spread" => HotCornerAction::WindowSpread,
        "launcher" => HotCornerAction::Launcher,
        "control-center" => HotCornerAction::ControlCenter,
        "settings" => HotCornerAction::Settings,
        _ => anyhow::bail!("触发角动作无效"),
    })
}

fn same_group(path: &[&str], left: &Config, right: &Config) -> bool {
    match path {
        ["input", "touchpad"] => left.input.touchpad == right.input.touchpad,
        ["input", "mouse"] => left.input.mouse == right.input.mouse,
        ["animations", name] => same_animation(name, &left.animations, &right.animations),
        _ => false,
    }
}

fn same_animation(name: &str, left: &Animations, right: &Animations) -> bool {
    match name {
        "window-open" => left.window_open == right.window_open,
        "window-close" => left.window_close == right.window_close,
        "window-movement" => left.window_movement == right.window_movement,
        "window-resize" => left.window_resize == right.window_resize,
        "workspace-switch" => left.workspace_switch == right.workspace_switch,
        "window-wobble" => left.window_wobble == right.window_wobble,
        _ => false,
    }
}

fn seed_group(
    source: &str,
    sources: &[(PathBuf, Option<Vec<u8>>)],
    path: &[&str],
    effective: &Config,
) -> Result<String> {
    ensure_unique(source, path)?;
    let mut matching = None;
    for candidate in std::iter::once(source.to_owned()).chain(
        sources
            .iter()
            .filter_map(|(_, bytes)| bytes.as_ref())
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned()),
    ) {
        let document = knuffel::parse_ast::<Span>("config.kdl", &candidate)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        for parent in document.nodes.iter().filter(|n| &**n.node_name == path[0]) {
            for node in parent.children().filter(|n| &**n.node_name == path[1]) {
                let span = node.span();
                let block = &candidate[span.0..span.1];
                let isolated = Config::parse_mem(&format!("{} {{\n{block}\n}}", path[0]))?;
                if same_group(path, &isolated, effective) {
                    matching = Some(block.to_owned());
                    break;
                }
            }
            if matching.is_some() {
                break;
            }
        }
        if matching.is_some() {
            break;
        }
    }
    let Some(block) = matching else {
        anyhow::ensure!(
            same_group(path, &Config::default(), effective),
            "{} 的完整配置来源无法确定，未覆盖其他参数",
            path.join(".")
        );
        return Ok(source.to_owned());
    };
    let mut editor = Editor::new(source)?;
    if let Some(current) = node_text(source, path)? {
        if current == block {
            return Ok(source.to_owned());
        }
    }
    editor.upsert(&path[..path.len() - 1], path[path.len() - 1], &block)?;
    Ok(editor.finish())
}

fn find_node<'a>(
    nodes: &'a [SpannedNode<Span>],
    path: &[&str],
) -> Result<Option<&'a SpannedNode<Span>>> {
    let Some((name, rest)) = path.split_first() else {
        return Ok(None);
    };
    let mut matches = nodes.iter().filter(|n| &**n.node_name == *name);
    let Some(node) = matches.next() else {
        return Ok(None);
    };
    anyhow::ensure!(
        matches.next().is_none(),
        "配置路径 {} 有重复节点，请在全部配置中合并",
        path.join(".")
    );
    if rest.is_empty() {
        Ok(Some(node))
    } else {
        find_node(
            node.children.as_deref().map(Vec::as_slice).unwrap_or(&[]),
            rest,
        )
    }
}

fn ensure_unique(source: &str, path: &[&str]) -> Result<()> {
    let document =
        knuffel::parse_ast::<Span>("config.kdl", source).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    find_node(&document.nodes, path)?;
    Ok(())
}

fn node_text(source: &str, path: &[&str]) -> Result<Option<String>> {
    let document =
        knuffel::parse_ast::<Span>("config.kdl", source).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    Ok(find_node(&document.nodes, path)?.map(|n| {
        let span = n.span();
        source[span.0..span.1].to_owned()
    }))
}

/// Replace only the argument token: properties, comments and exact unrelated
/// numeric precision survive slider edits (including scroll-factor axis props).
fn set_scalar(source: &str, path: &[&str], value: &str) -> Result<String> {
    let document =
        knuffel::parse_ast::<Span>("config.kdl", source).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    if let Some(node) = find_node(&document.nodes, path)? {
        anyhow::ensure!(
            node.arguments.len() <= 1,
            "设置 {} 有多个参数",
            path.join(".")
        );
        let mut result = source.to_owned();
        if let Some(arg) = node.arguments.first() {
            let span = arg.literal.span();
            result.replace_range(span.0..span.1, value);
        } else {
            let end = node.node_name.span().1;
            result.insert_str(end, &format!(" {value}"));
        }
        return Ok(result);
    }
    let (name, parent) = path.split_last().context("缺少设置路径")?;
    let mut editor = Editor::new(source)?;
    editor.upsert(parent, name, &format!("{name} {value}"))?;
    Ok(editor.finish())
}

fn remove(source: &str, path: &[&str]) -> Result<String> {
    ensure_unique(source, path)?;
    let mut editor = Editor::new(source)?;
    editor.remove(path)?;
    Ok(editor.finish())
}

fn set_bare_flag(source: &str, path: &[&str], value: bool) -> Result<String> {
    ensure_unique(source, path)?;
    let mut editor = Editor::new(source)?;
    if value {
        let (name, parent) = path.split_last().context("缺少开关路径")?;
        if !editor.exists(path) {
            editor.upsert(parent, name, name)?;
        }
    } else {
        editor.remove(path)?;
    }
    Ok(editor.finish())
}

fn set_switch(source: &str, path: &[&str], enabled: bool) -> Result<String> {
    let mut opposite = path.to_vec();
    opposite.push(if enabled { "off" } else { "on" });
    let content = remove(source, &opposite)?;
    let mut selected = path.to_vec();
    selected.push(if enabled { "on" } else { "off" });
    set_bare_flag(&content, &selected, true)
}

const GRADIENT_MEMORY: &str = "clarion-settings: active-border-gradient=";
const OLD_GRADIENT_SPEED_MEMORY: &str = "clarion-settings: active-border-gradient-speed=";
const GRADIENT_PATH: &[&str] = &["layout", "border", "active-gradient"];

fn relative_text(value: clarion_config::appearance::GradientRelativeTo) -> &'static str {
    match value {
        clarion_config::appearance::GradientRelativeTo::Window => "window",
        clarion_config::appearance::GradientRelativeTo::WorkspaceView => "workspace-view",
    }
}

fn interpolation_text(value: clarion_config::appearance::GradientInterpolation) -> String {
    use clarion_config::appearance::GradientColorSpace as Space;
    use clarion_config::appearance::HueInterpolation as Hue;
    match value.color_space {
        Space::Srgb => String::new(),
        Space::SrgbLinear => "srgb-linear".to_owned(),
        Space::Oklab => "oklab".to_owned(),
        Space::Oklch => match value.hue_interpolation {
            Hue::Shorter => "oklch".to_owned(),
            Hue::Longer => "oklch longer hue".to_owned(),
            Hue::Increasing => "oklch increasing hue".to_owned(),
            Hue::Decreasing => "oklch decreasing hue".to_owned(),
        },
    }
}

fn gradient_properties(gradient: clarion_config::Gradient) -> Result<String> {
    let mut properties = format!(
        "from={} to={} angle={} relative-to={} rotate-deg-per-sec={}",
        serde_json::to_string(&color_text(gradient.from))?,
        serde_json::to_string(&color_text(gradient.to))?,
        gradient.angle,
        serde_json::to_string(relative_text(gradient.relative_to))?,
        gradient.rotate_deg_per_sec.map_or(0., |speed| speed.0),
    );
    let interpolation = interpolation_text(gradient.in_);
    if !interpolation.is_empty() {
        properties.push_str(&format!(
            " in={}",
            serde_json::to_string(&interpolation)?
        ));
    }
    Ok(properties)
}

/// Match the complete effective gradient before copying a local override.
/// Inactive and urgent gradients, solid colors and border enablement stay intact.
fn matching_gradient(source: &str, effective: clarion_config::Gradient) -> Result<Option<String>> {
    let document =
        knuffel::parse_ast::<Span>("config.kdl", source).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    for layout in document
        .nodes
        .iter()
        .filter(|node| &**node.node_name == "layout")
    {
        for border in layout
            .children()
            .filter(|node| &**node.node_name == "border")
        {
            for node in border
                .children()
                .filter(|node| &**node.node_name == "active-gradient")
            {
                let span = node.span();
                let block = &source[span.0..span.1];
                let isolated = Config::parse_mem(&format!("layout {{ border {{ {block}
 }}; }}"))?;
                if isolated.layout.border.active_gradient == Some(effective) {
                    return Ok(Some(block.to_owned()));
                }
            }
        }
    }
    Ok(None)
}

/// Copy an inherited gradient into the edited file so property edits stay local.
fn prepare_gradient(
    source: &str,
    sources: &[(PathBuf, Option<Vec<u8>>)],
    desired: &mut Config,
) -> Result<String> {
    ensure_unique(source, GRADIENT_PATH)?;
    let gradient = desired
        .layout
        .border
        .active_gradient
        .context("请先启用活动边框渐变")?;
    let mut found = None;
    for candidate in std::iter::once(source.to_owned()).chain(
        sources
            .iter()
            .filter_map(|(_, bytes)| bytes.as_ref())
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned()),
    ) {
        if let Some(matching) = matching_gradient(&candidate, gradient)? {
            found = Some(matching);
            break;
        }
    }
    let block = found.context("活动边框渐变来源无法确定，未修改原配置")?;
    if node_text(source, GRADIENT_PATH)?.as_deref() == Some(block.as_str()) {
        return Ok(source.to_owned());
    }
    let mut editor = Editor::new(source)?;
    editor.upsert(&["layout", "border"], "active-gradient", &block)?;
    Ok(editor.finish())
}

fn create_gradient(
    source: &str,
    sources: &[(PathBuf, Option<Vec<u8>>)],
    desired: &mut Config,
    after: &CommonSettings,
) -> Result<String> {
    ensure_unique(source, GRADIENT_PATH)?;
    if let Some(properties) = std::iter::once(source.to_owned()).chain(
        sources
            .iter()
            .filter_map(|(_, bytes)| bytes.as_ref())
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned()),
    ).find_map(|candidate| gradient_memory(&candidate)) {
        let block = format!("active-gradient {properties}");
        let isolated = Config::parse_mem(&format!("layout {{ border {{ {block}; }}; }}"))?;
        let gradient = isolated
            .layout
            .border
            .active_gradient
            .context("活动边框渐变格式无效")?;
        let mut editor = Editor::new(source)?;
        editor.upsert(&["layout", "border"], "active-gradient", &block)?;
        let content = editor.finish();
        desired.layout.border.active_gradient = Some(gradient);
        return Ok(content);
    }
    fn literal(color: clarion_config::Color) -> String {
        format!(
            "rgba({}, {}, {}, {})",
            f64::from(color.r) * 255.,
            f64::from(color.g) * 255.,
            f64::from(color.b) * 255.,
            color.a
        )
    }
    let mut block = format!(
        "active-gradient from={} to={} angle=180 relative-to=\"window\" rotate-deg-per-sec=0",
        serde_json::to_string(&literal(desired.layout.border.active_color))?,
        serde_json::to_string(&literal(desired.layout.border.inactive_color))?,
    );
    let interpolation = after.border_gradient_in.trim();
    if !interpolation.is_empty() {
        block.push_str(&format!(
            " in={}",
            serde_json::to_string(interpolation)?
        ));
    }
    let isolated = Config::parse_mem(&format!("layout {{ border {{ {block}; }}; }}"))?;
    let gradient = isolated
        .layout
        .border
        .active_gradient
        .context("活动边框渐变格式无效")?;
    let mut editor = Editor::new(source)?;
    editor.upsert(&["layout", "border"], "active-gradient", &block)?;
    let content = editor.finish();
    desired.layout.border.active_gradient = Some(gradient);
    Ok(content)
}

fn set_gradient_rotation(source: &str, rotation: f64) -> Result<String> {
    set_gradient_property(source, "rotate-deg-per-sec", &rotation.to_string())
}

fn set_gradient_property(source: &str, name: &str, value: &str) -> Result<String> {
    let document =
        knuffel::parse_ast::<Span>("config.kdl", source).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let node = find_node(&document.nodes, GRADIENT_PATH)?.context("找不到活动边框渐变")?;
    let mut content = source.to_owned();
    if let Some((_, existing)) = node
        .properties
        .iter()
        .find(|(property, _)| &****property == name)
    {
        let span = existing.literal.span();
        content.replace_range(span.0..span.1, value);
    } else {
        content.insert_str(node.node_name.span().1, &format!(" {name}={value}"));
    }
    Ok(content)
}

/// Remember the removed gradient so re-enabling it (even after a restart) keeps
/// the colours, angle, scope and interpolation the user configured.
fn set_gradient_memory(source: &str, properties: &str) -> Result<String> {
    let text = format!("{GRADIENT_MEMORY}{properties}");
    let mut content = source.to_owned();
    let comments = real_line_comments(source);
    let span = comments
        .iter()
        .find(|span| {
            source[span.start + 2..span.end]
                .trim()
                .starts_with(GRADIENT_MEMORY)
        })
        .or_else(|| {
            comments.iter().find(|span| {
                source[span.start + 2..span.end]
                    .trim()
                    .starts_with(OLD_GRADIENT_SPEED_MEMORY)
            })
        });
    if let Some(span) = span {
        content.replace_range(span.clone(), &format!("// {text}"));
        return Ok(content);
    }
    let document =
        knuffel::parse_ast::<Span>("config.kdl", source).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let border = find_node(&document.nodes, &["layout", "border"])?.context("找不到边框设置")?;
    let start = border.node_name.span().0;
    let line_start = source[..start].rfind('\n').map_or(0, |n| n + 1);
    let prefix = &source[line_start..start];
    let indent = if prefix.trim().is_empty() { prefix } else { "" };
    content.insert_str(line_start, &format!("{indent}// {text}\n"));
    Ok(content)
}

fn gradient_memory(source: &str) -> Option<String> {
    real_line_comments(source).into_iter().find_map(|span| {
        let text = source[span.start + 2..span.end].trim();
        let properties = text.strip_prefix(GRADIENT_MEMORY)?;
        parse_gradient(properties).ok()?;
        Some(properties.to_owned())
    })
}

fn parse_gradient(properties: &str) -> Result<clarion_config::Gradient> {
    let isolated = Config::parse_mem(&format!(
        "layout {{ border {{ active-gradient {properties}; }}; }}"
    ))?;
    isolated
        .layout
        .border
        .active_gradient
        .context("活动边框渐变格式无效")
}

/// Identify actual line comments, skipping quoted/raw KDL strings and nested
/// block comments. A shader or startup argument containing a fake marker must
/// never become remembered settings data.
fn real_line_comments(source: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = source.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if bytes[i] == b'r' {
            let mut quote = i + 1;
            while quote < bytes.len() && bytes[quote] == b'#' {
                quote += 1;
            }
            if bytes.get(quote) == Some(&b'"') {
                let terminator = format!("\"{}", "#".repeat(quote - i - 1));
                i = source[quote + 1..]
                    .find(&terminator)
                    .map_or(bytes.len(), |end| quote + 1 + end + terminator.len());
            } else {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < bytes.len() && depth > 0 {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else if bytes[i..].starts_with(b"//") {
            let start = i;
            i = source[i..].find('\n').map_or(bytes.len(), |end| i + end);
            found.push(start..i);
        } else {
            i += 1;
        }
    }
    found
}

fn preserve_close_timing(source: &str, animation: Animation) -> Result<String> {
    let mut editor = Editor::new(source)?;
    let parent = &["animations", "window-close"];
    if ["duration-ms", "curve", "spring"].iter().any(|name| {
        let mut path = parent.to_vec();
        path.push(name);
        editor.exists(&path)
    }) {
        return Ok(source.to_owned());
    }
    match animation.kind {
        Kind::Easing(params) => {
            editor.upsert(
                parent,
                "duration-ms",
                &format!("duration-ms {}", params.duration_ms),
            )?;
            let curve = match params.curve {
                Curve::Linear => "\"linear\"".into(),
                Curve::EaseOutQuad => "\"ease-out-quad\"".into(),
                Curve::EaseOutCubic => "\"ease-out-cubic\"".into(),
                Curve::EaseOutExpo => "\"ease-out-expo\"".into(),
                Curve::CubicBezier(a, b, c, d) => format!("\"cubic-bezier\" {a} {b} {c} {d}"),
            };
            editor.upsert(parent, "curve", &format!("curve {curve}"))?;
        }
        Kind::Spring(params) => editor.upsert(
            parent,
            "spring",
            &format!(
                "spring damping-ratio={} stiffness={} epsilon={}",
                params.damping_ratio, params.stiffness, params.epsilon,
            ),
        )?,
        Kind::WorkspaceFeedback => anyhow::bail!("工作区反馈不能用作窗口关闭动画"),
    }
    Ok(editor.finish())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn manager(source: &str) -> (tempfile::TempDir, ConfigManager) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        fs::write(&path, source).unwrap();
        let manager = ConfigManager::from_path(&path).unwrap();
        (dir, manager)
    }

    #[test]
    fn idle_cursor_xkb_hot_corners_and_blur_roundtrip() {
        let (_dir, mut manager) = manager("");
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.keyboard_xkb_layout = "us,cn".into();
        after.keyboard_xkb_variant = "intl".into();
        after.keyboard_xkb_options = "grp:alt_shift_toggle,ctrl:nocaps".into();
        after.idle_enabled = true;
        after.idle_timeout = 1800;
        after.idle_blank_timeout = 3600;
        after.idle_action = "lock".into();
        after.idle_lock_command = "hyprlock".into();
        after.idle_lock_before_suspend = true;
        after.cursor_theme = "Bibata-Modern-Ice".into();
        after.cursor_size = 24;
        after.cursor_hide_when_typing = true;
        after.cursor_hide_after_ms = Some(300_000);
        after.hot_corners_enabled = true;
        after.hot_corner_top_left = true;
        after.hot_corner_top_right = true;
        after.hot_corner_top_left_action = "expo".into();
        after.hot_corner_top_right_action = "window-spread".into();
        after.hot_corner_bottom_left_action = "settings".into();
        after.hot_corner_bottom_right_action = "none".into();
        after.blur_noise = 0.04;
        after.blur_saturation = 1.8;
        apply(&mut manager, &before, &after).unwrap();
        assert_eq!(read(&manager).unwrap(), after);
        let content = manager.raw_content();
        assert!(content.contains("idle"));
        assert!(content.contains("lock-command \"hyprlock\""));
        assert!(content.contains("layout \"us,cn\""));
        assert!(content.contains("hot-corners"));
        assert!(content.contains("top-left-action \"expo\""));
        assert!(content.contains("bottom-right-action \"none\""));
        assert!(content.contains("noise 0.04"));
    }

    #[test]
    fn decoration_layout_and_overview_roundtrip() {
        let (_dir, mut manager) = manager("");
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.border_active_color = "#ff0000".into();
        after.border_gradient_enabled = true;
        after.border_gradient_speed = 30.;
        after.border_gradient_from = "#ff0000".into();
        after.border_gradient_to = "#0000ff".into();
        after.border_gradient_angle = 90.;
        after.border_gradient_relative = "workspace-view".into();
        after.border_inactive_color = "#808080".into();
        after.border_urgent_color = "#ff00ff".into();
        after.focus_ring_active_color = "#00ff00".into();
        after.shadow_color = "#000000cc".into();
        after.shadow_offset_x = 8.;
        after.shadow_offset_y = -6.;
        after.shadow_softness = 20.;
        after.shadow_spread = 3.;
        after.shadow_draw_behind = true;
        after.struts_left = 12.;
        after.struts_right = 14.;
        after.struts_top = 30.;
        after.struts_bottom = 4.;
        after.dim_inactive = true;
        after.dim_factor = f64::from(0.82_f32);
        after.layout_background_color = "#123456".into();
        after.layout_fullscreen_backdrop_color = "#000000".into();
        after.rounding_power = 3.;
        after.tab_indicator_enabled = false;
        after.tab_indicator_gap = 6.;
        after.tab_indicator_width = 8.;
        after.tab_indicator_corner_radius = 3.;
        after.tab_indicator_active_color = "#abcdef".into();
        after.insert_hint_enabled = false;
        after.insert_hint_color = "#fedcba".into();
        after.overview_style = "strip".into();
        after.overview_zoom = 0.42;
        after.overview_backdrop_color = "#101010".into();
        after.overview_shadow_enabled = true;
        after.overview_shadow_offset_x = 4.;
        after.overview_shadow_offset_y = 12.;
        after.overview_shadow_softness = 24.;
        after.overview_shadow_spread = 6.;
        after.overview_shadow_color = "#00000080".into();
        after.stage_manager_enabled = true;
        after.preset_column_widths = "25%,50%,960px".into();
        after.preset_window_heights = "30%,640px".into();
        apply(&mut manager, &before, &after).unwrap();
        assert_eq!(read(&manager).unwrap(), after);
        let content = manager.raw_content();
        assert!(content.contains("offset x=8 y=-6"));
        assert!(content.contains("struts"));
        assert!(content.contains("left 12"));
        assert!(!content.contains("top-left-action"));
        assert!(content.contains("overview"));
        assert!(content.contains("style \"strip\""));
        assert!(content.contains("preset-column-widths"));
        assert!(content.contains("proportion 0.25"));
        assert!(content.contains("fixed 960"));
        assert!(content.contains("workspace-shadow"));
        assert!(content.contains("offset x=4 y=12"));
        assert!(content.contains("stage-manager"));
        assert!(content.contains("active-gradient"));
        assert!(content.contains("angle=90"));
        assert!(content.contains("relative-to=\"workspace-view\""));
    }

    #[test]
    fn no_change_keeps_every_byte_and_precision() {
        let source = "// exact user settings\nlayout { gaps 6.500000000003; border { width 2.1234567890123; }; }\n";
        let (_dir, mut manager) = manager(source);
        let before = read(&manager).unwrap();
        apply(&mut manager, &before, &before).unwrap();
        assert_eq!(manager.raw_content(), source);
        assert!(!manager.is_dirty());
    }

    #[test]
    fn only_changed_scalar_is_written_and_inline_comment_survives() {
        let source =
            "layout { gaps /* fine */ 6.500000000003; border { width 2.1234567890123; }; }\n";
        let (_dir, mut manager) = manager(source);
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.gaps = 9.;
        apply(&mut manager, &before, &after).unwrap();
        assert_eq!(manager.raw_content(), source.replace("6.500000000003", "9"));
        assert_eq!(read(&manager).unwrap(), after);
    }

    #[test]
    fn inherited_animation_toggle_keeps_shader_curve_and_reenables() {
        let (dir, mut manager) = manager("include \"base.kdl\"\n");
        fs::write(dir.path().join("base.kdl"), "animations { window-open { // retained curve\n duration-ms 713; curve \"cubic-bezier\" 0.12 0.234567890123 0.8 1.0; popin 0.91; custom-shader \"kept shader\"; glide { off; tilt-deg 7; }; }; }\n").unwrap();
        let original = manager.effective_config().unwrap().animations.window_open;
        let before = read(&manager).unwrap();
        let mut off = before.clone();
        off.window_open_enabled = false;
        apply(&mut manager, &before, &off).unwrap();
        assert!(manager.raw_content().contains("// retained curve"));
        assert!(manager.raw_content().contains("0.234567890123"));
        apply(&mut manager, &off, &before).unwrap();
        assert_eq!(
            manager.effective_config().unwrap().animations.window_open,
            original
        );
    }

    #[test]
    fn inherited_touchpad_and_mouse_keep_advanced_fields() {
        let (dir, mut manager) = manager("include \"input.kdl\"\n");
        fs::write(dir.path().join("input.kdl"), "input { touchpad { tap; dwt; natural-scroll; accel-speed 0.1234567890123; click-method \"clickfinger\"; left-handed; scroll-factor 1.1 horizontal=0.8 vertical=1.4; }; mouse { natural-scroll; middle-emulation; accel-profile \"flat\"; accel-speed 0.234567890123; }; }\n").unwrap();
        let before_config = manager.effective_config().unwrap();
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.touchpad_tap = false;
        after.mouse_natural_scroll = false;
        apply(&mut manager, &before, &after).unwrap();
        let actual = manager.effective_config().unwrap();
        let mut desired_touchpad = before_config.input.touchpad;
        desired_touchpad.tap = false;
        let mut desired_mouse = before_config.input.mouse;
        desired_mouse.natural_scroll = false;
        assert_eq!(actual.input.touchpad, desired_touchpad);
        assert_eq!(actual.input.mouse, desired_mouse);
        assert!(manager.raw_content().contains("0.1234567890123"));
    }

    #[test]
    fn flag_false_overrides_inherited_true_without_other_overrides() {
        let (dir, mut manager) = manager("include \"base.kdl\"\n");
        fs::write(dir.path().join("base.kdl"), "layout { auto-stack-new-windows; always-center-single-column; }\ninput { keyboard { numlock; }; }\n").unwrap();
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.auto_stack = false;
        after.center_single = false;
        after.keyboard_numlock = false;
        apply(&mut manager, &before, &after).unwrap();
        for flag in [
            "auto-stack-new-windows false",
            "always-center-single-column false",
            "numlock false",
        ] {
            assert!(manager.raw_content().contains(flag));
        }
        assert_eq!(read(&manager).unwrap(), after);
    }

    #[test]
    fn later_include_override_is_error_without_draft_mutation() {
        let (dir, mut manager) = manager("layout { gaps 5; }\ninclude \"later.kdl\"\n");
        fs::write(dir.path().join("later.kdl"), "layout { gaps 9; }\n").unwrap();
        let original = manager.raw_content().to_owned();
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.gaps = 12.;
        assert!(apply(&mut manager, &before, &after).is_err());
        assert_eq!(manager.raw_content(), original);
    }

    #[test]
    fn stale_changed_field_rejected_but_unrelated_external_field_kept() {
        let (dir, mut manager) = manager("include \"base.kdl\"\n");
        let included = dir.path().join("base.kdl");
        fs::write(&included, "layout { gaps 5; rounding 12; }\n").unwrap();
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.gaps = 12.;
        fs::write(&included, "layout { gaps 7; rounding 12; }\n").unwrap();
        assert!(apply(&mut manager, &before, &after).is_err());
        fs::write(&included, "layout { gaps 5; rounding 22; }\n").unwrap();
        apply(&mut manager, &before, &after).unwrap();
        assert_eq!(manager.effective_config().unwrap().layout.rounding, 22.);
    }

    #[test]
    fn scroll_base_preserves_per_axis_overrides() {
        let (_dir, mut manager) =
            manager("input { touchpad { scroll-factor 1.1 horizontal=0.8 vertical=1.4; }; }\n");
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.touchpad_scroll_factor = 1.7;
        apply(&mut manager, &before, &after).unwrap();
        assert!(manager
            .raw_content()
            .contains("scroll-factor 1.7 horizontal=0.8 vertical=1.4"));
    }

    #[test]
    fn invalid_values_keep_original_draft() {
        let (_dir, mut manager) = manager("// unchanged\n");
        let before = read(&manager).unwrap();
        for value in [f64::NAN, f64::INFINITY, -3.] {
            let mut after = before.clone();
            after.gaps = value;
            assert!(apply(&mut manager, &before, &after).is_err());
            assert_eq!(manager.raw_content(), "// unchanged\n");
        }
    }

    #[test]
    fn switching_builtin_effect_keeps_current_timing() {
        let (_dir, mut manager) = manager("animations { window-close { effect \"implode\"; }; }\n");
        let original = manager
            .effective_config()
            .unwrap()
            .animations
            .window_close
            .anim;
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.window_close_effect = "fade".into();
        apply(&mut manager, &before, &after).unwrap();
        assert_eq!(
            manager
                .effective_config()
                .unwrap()
                .animations
                .window_close
                .anim,
            original
        );
        assert_eq!(read(&manager).unwrap(), after);
    }

    #[test]
    fn common_controls_round_trip_without_resetting_other_sections() {
        let (_dir, mut manager) = manager("// keep startup command\nspawn-at-startup \"foot\"\nlayout { background-color \"#112233\"; }\ninput { keyboard { xkb { layout \"us\"; }; }; }\n");
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.animations_enabled = false;
        after.animation_speed = 0.8;
        after.window_open_enabled = false;
        after.window_close_enabled = false;
        after.window_movement_enabled = false;
        after.window_resize_enabled = false;
        after.workspace_switch_enabled = false;
        after.window_open_effect = "reveal".into();
        after.window_close_effect = "fade".into();
        after.wobble_enabled = false;
        after.cube_enabled = false;
        after.glide_enabled = true;
        after.auto_stack = false;
        after.max_windows_per_column = 5;
        after.default_width = Some(PresetSize::Fixed(900));
        after.gaps = 8.5;
        after.center_column = "on-overflow".into();
        after.center_single = true;
        after.border_enabled = true;
        after.focus_ring_enabled = false;
        after.shadow_enabled = true;
        after.border_width = 3.;
        after.focus_ring_width = 2.;
        after.rounding = 12.;
        after.blur_enabled = false;
        after.blur_passes = 4;
        after.blur_offset = 2.;
        after.touchpad_tap = true;
        after.touchpad_natural_scroll = true;
        after.touchpad_dwt = true;
        after.touchpad_speed = 0.25;
        after.touchpad_scroll_factor = 1.25;
        after.mouse_natural_scroll = true;
        after.mouse_speed = -0.25;
        after.keyboard_numlock = true;
        after.keyboard_repeat_rate = 35;
        after.keyboard_repeat_delay = 300;
        apply(&mut manager, &before, &after).unwrap();
        assert_eq!(read(&manager).unwrap(), after);
        assert!(manager.raw_content().contains("spawn-at-startup \"foot\""));
        assert!(manager.raw_content().contains("// keep startup command"));
        assert!(manager
            .raw_content()
            .contains("background-color \"#112233\""));
        assert!(manager.raw_content().contains("layout \"us\""));
        // Exercise the opposite direction as well: explicit false Flags and
        // paired on/off nodes must restore, rather than latch the disabled state.
        apply(&mut manager, &after, &before).unwrap();
        assert_eq!(read(&manager).unwrap(), before);
    }

    #[test]
    fn duplicate_edited_blocks_fail_without_touching_draft() {
        let source = "layout { gaps 5; }\nlayout { rounding 12; }\n";
        let (_dir, mut manager) = manager(source);
        assert!(read(&manager).is_err());
        assert!(ensure_unique(source, &["layout", "gaps"]).is_err());
        // The compositor rejects duplicate singleton blocks before structured
        // edits are possible. A stale previously valid form must also fail.
        let before = CommonSettings::default();
        let mut after = before.clone();
        after.gaps = 9.;
        assert!(apply(&mut manager, &before, &after).is_err());
        assert_eq!(manager.raw_content(), source);
    }

    #[test]
    fn same_builtin_effect_can_explicitly_replace_shader_and_false_to_true_is_rejected() {
        let source = "animations { window-open { effect \"fade\"; custom-shader \"custom open\"; }; window-close { effect \"fade\"; custom-shader \"custom close\"; duration-ms 763; }; }\n";
        let (_dir, mut manager) = manager(source);
        let before = read(&manager).unwrap();
        assert!(before.open_custom_shader && before.close_custom_shader);
        let mut after = before.clone();
        after.open_custom_shader = false;
        after.close_custom_shader = false;
        apply(&mut manager, &before, &after).unwrap();
        assert_eq!(read(&manager).unwrap(), after);
        assert!(manager.raw_content().contains("duration-ms 763"));
        let replaced = manager.raw_content().to_owned();
        assert!(apply(&mut manager, &after, &before).is_err());
        assert_eq!(manager.raw_content(), replaced);
    }

    #[test]
    fn system_and_advanced_input_roundtrip() {
        let source = "input { touchpad { tap; }; }\nprefer-no-csd\n";
        let (_dir, mut manager) = manager(source);
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.touchpad_drag = true;
        after.touchpad_drag_lock = true;
        after.touchpad_click_method = "clickfinger".into();
        after.touchpad_accel_profile = "flat".into();
        after.touchpad_scroll_method = "two-finger".into();
        after.touchpad_tap_button_map = "left-middle-right".into();
        after.touchpad_left_handed = true;
        after.touchpad_middle_emulation = true;
        after.touchpad_disable_on_external_mouse = true;
        after.touchpad_dwtp = true;
        after.mouse_accel_profile = "adaptive".into();
        after.mouse_scroll_method = "edge".into();
        after.mouse_left_handed = true;
        after.mouse_middle_emulation = true;
        after.trackpoint_enabled = false;
        after.trackpoint_natural_scroll = true;
        after.trackpoint_speed = 0.25;
        after.trackpoint_accel_profile = "flat".into();
        after.trackpoint_left_handed = true;
        after.trackpoint_middle_emulation = true;
        after.screenshot_path = "/tmp/shot-%Y.png".into();
        after.xwayland_enabled = false;
        after.xwayland_path = "/usr/bin/Xwayland".into();
        after.prefer_no_csd = true;
        after.clipboard_disable_primary = true;
        after.hotkey_skip_at_startup = true;
        after.hotkey_hide_not_bound = true;
        after.config_notification_disable_failed = true;
        apply(&mut manager, &before, &after).unwrap();
        assert_eq!(read(&manager).unwrap(), after);
        let content = manager.raw_content().to_owned();
        for needle in [
            "click-method",
            "tap-button-map",
            "accel-profile",
            "scroll-method",
            "drag true",
            "disabled-on-external-mouse",
            "dwtp",
            "trackpoint",
            "screenshot-path",
            "xwayland-satellite",
            "prefer-no-csd",
            "clipboard",
            "hotkey-overlay",
            "config-notification",
        ] {
            assert!(content.contains(needle), "missing {needle}: {content}");
        }

        let mut cleared = after.clone();
        cleared.touchpad_click_method.clear();
        cleared.touchpad_accel_profile.clear();
        cleared.mouse_accel_profile.clear();
        cleared.touchpad_scroll_method.clear();
        cleared.mouse_scroll_method.clear();
        cleared.trackpoint_accel_profile.clear();
        apply(&mut manager, &after, &cleared).unwrap();
        assert_eq!(read(&manager).unwrap(), cleared);
        let content = manager.raw_content();
        assert!(!content.contains("click-method"));
        assert!(!content.contains("accel-profile"), "{content}");
        assert!(!content.contains("scroll-method"));
    }

    #[test]
    fn gradient_toggle_across_fresh_managers_preserves_custom_colors_and_speed() {
        let source = "layout { border {\n // custom active gradient\n active-gradient from=\"rgba(12.125, 34.5, 56.75, 0.731)\" to=\"#abcdef91\" angle=271 relative-to=\"workspace-view\" in=\"oklch longer hue\" rotate-deg-per-sec=-63.123456789\n inactive-gradient from=\"#111111\" to=\"#222222\" rotate-deg-per-sec=17\n urgent-color \"#ff9955\"\n }; }\n";
        let (dir, mut manager) = manager(source);
        let original = manager.effective_config().unwrap().layout.border;
        let before = read(&manager).unwrap();
        assert!(before.border_gradient_enabled);
        let mut off = before.clone();
        off.border_gradient_enabled = false;
        apply(&mut manager, &before, &off).unwrap();
        let without = manager.effective_config().unwrap().layout.border;
        assert!(without.active_gradient.is_none());
        assert_eq!(without.active_color, original.active_color);
        assert_eq!(without.inactive_color, original.inactive_color);
        assert_eq!(without.inactive_gradient, original.inactive_gradient);
        assert_eq!(without.urgent_color, original.urgent_color);
        assert!(manager.raw_content().contains("// custom active gradient"));
        let raw = manager.raw_content().to_owned();
        assert!(raw.contains("clarion-settings: active-border-gradient=from="));
        assert!(raw.contains("in=\"oklch longer hue\""));
        assert!(raw.contains("rotate-deg-per-sec=-63.123456789"));
        let path = dir.path().join("config.kdl");
        fs::write(&path, raw).unwrap();
        let mut fresh = ConfigManager::from_path(&path).unwrap();
        let remembered = read(&fresh).unwrap();
        assert_eq!(remembered, off);
        apply(&mut fresh, &remembered, &before).unwrap();
        assert_eq!(fresh.effective_config().unwrap().layout.border, original);
        assert_eq!(read(&fresh).unwrap(), before);
    }

    #[test]
    fn inherited_gradient_stays_in_its_source_file_and_local_edits_materialize() {
        let (dir, mut manager) = manager("include \"base.kdl\"\n");
        fs::write(dir.path().join("base.kdl"), "layout { border { off; width 2.123456789; active-gradient from=\"#123456\" to=\"#654321\" angle=75 relative-to=\"workspace-view\" in=\"srgb-linear\" rotate-deg-per-sec=120; inactive-color \"#13579b\"; }; }\n").unwrap();
        let original = manager.effective_config().unwrap().layout.border;
        let before = read(&manager).unwrap();
        assert!(before.border_gradient_enabled);
        let mut disabled = before.clone();
        disabled.border_gradient_enabled = false;
        assert!(apply(&mut manager, &before, &disabled).is_err());
        assert_eq!(read(&manager).unwrap(), before);
        let mut slowed = before.clone();
        slowed.border_gradient_speed = 60.;
        apply(&mut manager, &before, &slowed).unwrap();
        assert_eq!(read(&manager).unwrap(), slowed);
        let actual = manager.effective_config().unwrap().layout.border;
        assert_eq!(
            actual.active_gradient.unwrap().rotate_deg_per_sec,
            Some(clarion_config::FloatOrInt(60.))
        );
        assert_eq!(actual.active_color, original.active_color);
        assert_eq!(actual.inactive_color, original.inactive_color);
        assert_eq!(actual.width, original.width);
        assert!(actual.off);
        assert!(manager.raw_content().contains("active-gradient"));
    }

    #[test]
    fn generated_gradient_uses_effective_colors_and_starts_static() {
        let (_dir, mut manager) = manager("layout { border { off; active-color \"rgba(17.125, 33.5, 91.25, 0.713)\"; inactive-color \"#12345687\"; urgent-gradient from=\"#f00\" to=\"#0f0\"; }; }\n");
        let original = manager.effective_config().unwrap().layout.border;
        let before = read(&manager).unwrap();
        assert!(!before.border_gradient_enabled);
        let mut after = before.clone();
        after.border_gradient_enabled = true;
        apply(&mut manager, &before, &after).unwrap();
        let actual = manager.effective_config().unwrap().layout.border;
        assert!(actual.off);
        let gradient = actual.active_gradient.unwrap();
        assert_eq!(gradient.from, original.active_color);
        assert_eq!(gradient.to, original.inactive_color);
        assert_eq!(gradient.rotate_deg_per_sec, Some(clarion_config::FloatOrInt(0.)));
        assert_eq!(actual.urgent_gradient, original.urgent_gradient);
        assert_eq!(actual.active_color, original.active_color);
        assert_eq!(actual.inactive_color, original.inactive_color);
        assert_eq!(read(&manager).unwrap(), after);
    }

    #[test]
    fn gradient_memory_only_reads_real_line_comments() {
        let source = r###"spawn-at-startup r##"// clarion-settings: active-border-gradient=from=#f00 to=#00f"##
animations { window-open { custom-shader "// clarion-settings: active-border-gradient=from=#0f0 to=#00f"; }; }
/* nested /* // clarion-settings: active-border-gradient=from=#0f0 to=#00f */ comment */
layout { border {
// clarion-settings: active-border-gradient=from="#123456" to="#654321" angle=75 relative-to="workspace-view" in="oklch longer hue" rotate-deg-per-sec=-23.456789
}; }
"###;
        let (_dir, manager) = manager(source);
        let settings = read(&manager).unwrap();
        assert!(!settings.border_gradient_enabled);
        assert_eq!(settings.border_gradient_speed, -23.456789);
        assert_eq!(settings.border_gradient_from, "#123456");
        assert_eq!(settings.border_gradient_to, "#654321");
        assert_eq!(settings.border_gradient_angle, 75.);
        assert_eq!(settings.border_gradient_relative, "workspace-view");
        assert_eq!(settings.border_gradient_in, "oklch longer hue");
        assert_eq!(real_line_comments(source).len(), 1);
    }

    #[test]
    fn materializing_border_width_keeps_inherited_disabled_state() {
        let (dir, mut manager) = manager("include \"base.kdl\"\n");
        fs::write(
            dir.path().join("base.kdl"),
            "layout { border { off; active-color \"#123456\"; }; }\n",
        )
        .unwrap();
        let before = read(&manager).unwrap();
        let mut after = before.clone();
        after.border_width = 7.5;
        apply(&mut manager, &before, &after).unwrap();
        assert_eq!(read(&manager).unwrap(), after);
        let actual = manager.effective_config().unwrap().layout.border;
        assert!(actual.off);
        assert_eq!(actual.width, 7.5);
        assert_eq!(actual.active_color, "#123456".parse().unwrap());
    }
}
