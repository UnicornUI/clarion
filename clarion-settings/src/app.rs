//! The native settings window: sidebar navigation, a detail header and
//! shell-owned desktop pages rendered with GPUI.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::str::FromStr;
use std::time::{Duration, Instant};

use clarion_config::output::OutputName;
use clarion_config::PresetSize;
use clarion_preferences::{
    DockPosition, LauncherPresentation, MinimizeEffect, Preferences, MAX_SURFACE_OPACITY,
    MIN_SURFACE_OPACITY,
};
use clarion_settings_core::common_settings::{
    parse_preset_sizes, preset_sizes_text, CommonSettings,
};
use clarion_settings_core::config_manager::ConfigManager;
use clarion_settings_core::ipc::Connection;
use clarion_ipc::Transform;
use clarion_settings_core::output_settings::{OutputSettings, VrrMode};
use gpui::prelude::*;
use gpui::{
    div, fill, img, linear_color_stop, linear_gradient, point, px, rgb, rgba, size, AnyElement,
    App, Background, Image, ImageFormat, ImageSource, ObjectFit, PathPromptOptions,
    AnyWindowHandle, Bounds, BoxShadow, Context, Entity, FocusHandle, FontWeight, Hsla,
    LayerShellAnchor, LayerShellKeyboardInteractivity, LayerShellLayer, LayerShellMargins,
    LayerShellOptions, MouseButton, MouseMoveEvent, Pixels, Point, Render, ScrollHandle,
    SharedString, WeakEntity,
    Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};

use crate::bridge::ShellBridge;
use crate::input::{QueryChanged, TextInput};
use crate::nav::{self, NavItem, Pane};
use crate::theme::{self, Palette};
use crate::widgets;

/// Height of the pinned window-decoration preview under the page header.
const WINDOW_PREVIEW_HEIGHT: f32 = 176.;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DropdownId {
    DockSize,
    DockPosition,
    Launcher,
    MinimizeEffect,
    CenterColumn,
    DefaultWidth,
    OpenEffect,
    CloseEffect,
    IdleAction,
    CursorHide,
    HotCornerTopLeft,
    HotCornerTopRight,
    HotCornerBottomLeft,
    HotCornerBottomRight,
    GradientRelative,
    TouchpadClick,
    TouchpadTapMap,
    TouchpadAccel,
    TouchpadScroll,
    MouseAccel,
    MouseScroll,
    TrackpointAccel,
    SystemTheme,
    IconTheme,
    CursorTheme,
    OverviewStyle,
    DisplayMaxBpc,
    DisplayResolution,
    DisplayRefresh,
    DisplayScale,
    DisplayTransform,
    DisplayVrr,
}

#[derive(Clone)]
enum DropdownChoice {
    Size(f64),
    Position(DockPosition),
    Launcher(LauncherPresentation),
    Effect(MinimizeEffect),
    Center(&'static str),
    Width(Option<PresetSize>),
    OpenEffect(&'static str),
    CloseEffect(&'static str),
    IdleAction(&'static str),
    CursorHide(Option<u32>),
    HotCornerAction(&'static str, &'static str),
    GradientRelative(&'static str),
    CommonText(DropdownId, &'static str),
    SystemTheme(String),
    IconTheme(String),
    CursorTheme(String),
    OverviewStyle(&'static str),
    MaxBpc(Option<u8>),
    Resolution(Option<(u16, u16)>),
    Refresh(Option<f64>),
    DisplayScale(Option<f64>),
    Transform(&'static str),
    Vrr(&'static str),
}

#[derive(Clone, PartialEq, Eq)]
enum ChoiceTarget {
    Shell,
    Common,
    Display,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TextKey {
    XkbLayout,
    XkbVariant,
    XkbOptions,
    IdleLockCommand,
    ScreenshotPath,
    XwaylandPath,
}

impl TextKey {
    const ALL: [Self; 6] = [
        Self::XkbLayout,
        Self::XkbVariant,
        Self::XkbOptions,
        Self::IdleLockCommand,
        Self::ScreenshotPath,
        Self::XwaylandPath,
    ];

    fn value(self, commons: &CommonSettings) -> String {
        match self {
            Self::XkbLayout => commons.keyboard_xkb_layout.clone(),
            Self::XkbVariant => commons.keyboard_xkb_variant.clone(),
            Self::XkbOptions => commons.keyboard_xkb_options.clone(),
            Self::IdleLockCommand => commons.idle_lock_command.clone(),
            Self::ScreenshotPath => commons.screenshot_path.clone(),
            Self::XwaylandPath => commons.xwayland_path.clone(),
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Self::XkbLayout => "自动（系统 locale）",
            Self::XkbVariant => "例如 intl",
            Self::XkbOptions => "例如 grp:alt_shift_toggle,ctrl:nocaps",
            Self::IdleLockCommand => "例如 hyprlock",
            Self::ScreenshotPath => "~/Pictures/Screenshots/%Y-%m-%d.png",
            Self::XwaylandPath => "xwayland-satellite",
        }
    }
}

#[derive(Clone)]
struct ThemeFamily {
    name: String,
    light: Option<String>,
    dark: Option<String>,
}

impl ThemeFamily {
    fn variant(&self, dark: bool) -> String {
        if dark {
            self.dark
                .clone()
                .or_else(|| self.light.clone())
                .unwrap_or_else(|| self.name.clone())
        } else {
            self.light
                .clone()
                .or_else(|| self.dark.clone())
                .unwrap_or_else(|| self.name.clone())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorKey {
    BorderActive,
    BorderInactive,
    BorderUrgent,
    BorderGradientFrom,
    BorderGradientTo,
    FocusActive,
    FocusInactive,
    FocusUrgent,
    Shadow,
    ShadowInactive,
    Background,
    FullscreenBackdrop,
    TabActive,
    TabInactive,
    TabUrgent,
    InsertHint,
    OverviewBackdrop,
    OverviewShadow,
}

impl ColorKey {
    fn id(self) -> &'static str {
        match self {
            Self::BorderActive => "color-border-active",
            Self::BorderInactive => "color-border-inactive",
            Self::BorderUrgent => "color-border-urgent",
            Self::BorderGradientFrom => "color-border-gradient-from",
            Self::BorderGradientTo => "color-border-gradient-to",
            Self::FocusActive => "color-focus-active",
            Self::FocusInactive => "color-focus-inactive",
            Self::FocusUrgent => "color-focus-urgent",
            Self::Shadow => "color-shadow",
            Self::ShadowInactive => "color-shadow-inactive",
            Self::Background => "color-background",
            Self::FullscreenBackdrop => "color-fullscreen-backdrop",
            Self::TabActive => "color-tab-active",
            Self::TabInactive => "color-tab-inactive",
            Self::TabUrgent => "color-tab-urgent",
            Self::InsertHint => "color-insert-hint",
            Self::OverviewBackdrop => "color-overview-backdrop",
            Self::OverviewShadow => "color-overview-shadow",
        }
    }

    fn value(self, commons: &CommonSettings) -> String {
        match self {
            Self::BorderActive => commons.border_active_color.clone(),
            Self::BorderInactive => commons.border_inactive_color.clone(),
            Self::BorderUrgent => commons.border_urgent_color.clone(),
            Self::BorderGradientFrom => commons.border_gradient_from.clone(),
            Self::BorderGradientTo => commons.border_gradient_to.clone(),
            Self::FocusActive => commons.focus_ring_active_color.clone(),
            Self::FocusInactive => commons.focus_ring_inactive_color.clone(),
            Self::FocusUrgent => commons.focus_ring_urgent_color.clone(),
            Self::Shadow => commons.shadow_color.clone(),
            Self::ShadowInactive => commons.shadow_inactive_color.clone(),
            Self::Background => commons.layout_background_color.clone(),
            Self::FullscreenBackdrop => commons.layout_fullscreen_backdrop_color.clone(),
            Self::TabActive => commons.tab_indicator_active_color.clone(),
            Self::TabInactive => commons.tab_indicator_inactive_color.clone(),
            Self::TabUrgent => commons.tab_indicator_urgent_color.clone(),
            Self::InsertHint => commons.insert_hint_color.clone(),
            Self::OverviewBackdrop => commons.overview_backdrop_color.clone(),
            Self::OverviewShadow => commons.overview_shadow_color.clone(),
        }
    }

    fn set(self, commons: &mut CommonSettings, value: String) {
        match self {
            Self::BorderActive => commons.border_active_color = value,
            Self::BorderInactive => commons.border_inactive_color = value,
            Self::BorderUrgent => commons.border_urgent_color = value,
            Self::BorderGradientFrom => commons.border_gradient_from = value,
            Self::BorderGradientTo => commons.border_gradient_to = value,
            Self::FocusActive => commons.focus_ring_active_color = value,
            Self::FocusInactive => commons.focus_ring_inactive_color = value,
            Self::FocusUrgent => commons.focus_ring_urgent_color = value,
            Self::Shadow => commons.shadow_color = value,
            Self::ShadowInactive => commons.shadow_inactive_color = value,
            Self::Background => commons.layout_background_color = value,
            Self::FullscreenBackdrop => commons.layout_fullscreen_backdrop_color = value,
            Self::TabActive => commons.tab_indicator_active_color = value,
            Self::TabInactive => commons.tab_indicator_inactive_color = value,
            Self::TabUrgent => commons.tab_indicator_urgent_color = value,
            Self::InsertHint => commons.insert_hint_color = value,
            Self::OverviewBackdrop => commons.overview_backdrop_color = value,
            Self::OverviewShadow => commons.overview_shadow_color = value,
        }
    }

    fn allows_empty(self) -> bool {
        matches!(
            self,
            Self::ShadowInactive | Self::TabActive | Self::TabInactive | Self::TabUrgent
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PresetList {
    ColumnWidths,
    WindowHeights,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PresetSelection {
    list: PresetList,
    index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorPickerTarget {
    Common(ColorKey),
    Accent,
}

impl ColorPickerTarget {
    fn allows_empty(self) -> bool {
        matches!(self, Self::Common(key) if key.allows_empty())
    }
}

#[derive(Clone)]
struct ColorPickerState {
    target: ColorPickerTarget,
    hue: f32,
    saturation: f32,
    value: f32,
    alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorDrag {
    Sv,
    Hue,
    Alpha,
}

impl DropdownChoice {
    fn selected(&self, app: &SettingsApp) -> bool {
        match self {
            Self::Size(value) => (app.prefs.dock_size - value).abs() < 0.5,
            Self::Position(value) => app.prefs.dock_position == *value,
            Self::Launcher(value) => app.prefs.launcher_presentation == *value,
            Self::Effect(value) => app.prefs.minimize_effect == *value,
            Self::Center(value) => app.commons_draft.center_column == *value,
            Self::Width(value) => app.commons_draft.default_width == *value,
            Self::OpenEffect(value) => app.commons_draft.window_open_effect == *value,
            Self::CloseEffect(value) => app.commons_draft.window_close_effect == *value,
            Self::IdleAction(value) => app.commons_draft.idle_action == *value,
            Self::CursorHide(value) => app.commons_draft.cursor_hide_after_ms == *value,
            Self::GradientRelative(value) => {
                app.commons_draft.border_gradient_relative == *value
            }
            Self::SystemTheme(value) => app.system_theme_family == *value,
            Self::IconTheme(value) => app.icon_theme_family == *value,
            Self::CursorTheme(value) => app.cursor_theme_family == *value,
            Self::CommonText(id, value) => match id {
                DropdownId::TouchpadClick => {
                    app.commons_draft.touchpad_click_method == *value
                }
                DropdownId::TouchpadTapMap => {
                    app.commons_draft.touchpad_tap_button_map == *value
                }
                DropdownId::TouchpadAccel => {
                    app.commons_draft.touchpad_accel_profile == *value
                }
                DropdownId::TouchpadScroll => {
                    app.commons_draft.touchpad_scroll_method == *value
                }
                DropdownId::MouseAccel => app.commons_draft.mouse_accel_profile == *value,
                DropdownId::MouseScroll => app.commons_draft.mouse_scroll_method == *value,
                DropdownId::TrackpointAccel => {
                    app.commons_draft.trackpoint_accel_profile == *value
                }
                _ => false,
            },
            Self::OverviewStyle(value) => app.commons_draft.overview_style == *value,
            Self::HotCornerAction(corner, value) => match *corner {
                "top-left" => app.commons_draft.hot_corner_top_left_action == *value,
                "top-right" => app.commons_draft.hot_corner_top_right_action == *value,
                "bottom-left" => app.commons_draft.hot_corner_bottom_left_action == *value,
                _ => app.commons_draft.hot_corner_bottom_right_action == *value,
            },
            Self::Resolution(size) => app
                .display
                .as_ref()
                .is_some_and(|page| {
                    !page.draft.custom_modeline
                        && page.draft.mode.map(|mode| (mode.width, mode.height)) == *size
                }),
            Self::Refresh(rate) => app.display.as_ref().is_some_and(|page| {
                !page.draft.custom_modeline
                    && match (page.draft.mode.and_then(|mode| mode.refresh), rate) {
                        (None, None) => true,
                        (Some(left), Some(right)) => (left - right).abs() < 0.01,
                        _ => false,
                    }
            }),
            Self::DisplayScale(scale) => app
                .display
                .as_ref()
                .is_some_and(|page| page.draft.scale == *scale),
            Self::Transform(key) => app
                .display
                .as_ref()
                .is_some_and(|page| transform_key(page.draft.transform) == *key),
            Self::Vrr(key) => app
                .display
                .as_ref()
                .is_some_and(|page| vrr_key(page.draft.vrr) == *key),
            Self::MaxBpc(value) => app
                .display
                .as_ref()
                .is_some_and(|page| page.draft.max_bpc == *value),
        }
    }

    fn apply(&self, app: &mut SettingsApp) -> ChoiceTarget {
        match self {
            Self::Size(value) => {
                app.prefs.dock_size = *value;
                return ChoiceTarget::Shell;
            }
            Self::Position(value) => {
                app.prefs.dock_position = *value;
                return ChoiceTarget::Shell;
            }
            Self::Launcher(value) => {
                app.prefs.launcher_presentation = *value;
                return ChoiceTarget::Shell;
            }
            Self::Effect(value) => {
                app.prefs.minimize_effect = *value;
                return ChoiceTarget::Shell;
            }
            Self::Center(value) => app.commons_draft.center_column = (*value).to_owned(),
            Self::Width(value) => app.commons_draft.default_width = *value,
            Self::OpenEffect(value) => {
                app.commons_draft.window_open_effect = (*value).to_owned();
                app.commons_draft.open_custom_shader = false;
            }
            Self::CloseEffect(value) => {
                app.commons_draft.window_close_effect = (*value).to_owned();
                app.commons_draft.close_custom_shader = false;
            }
            Self::IdleAction(value) => app.commons_draft.idle_action = (*value).to_owned(),
            Self::CursorHide(value) => app.commons_draft.cursor_hide_after_ms = *value,
            Self::GradientRelative(value) => {
                app.commons_draft.border_gradient_relative = (*value).to_owned()
            }
            Self::SystemTheme(value) => {
                app.set_system_theme(value);
                return ChoiceTarget::Shell;
            }
            Self::IconTheme(value) => {
                app.set_icon_theme(value);
                return ChoiceTarget::Shell;
            }
            Self::CursorTheme(value) => {
                app.set_cursor_theme(value);
                return ChoiceTarget::Common;
            }
            Self::CommonText(id, value) => {
                match id {
                    DropdownId::TouchpadClick => {
                        app.commons_draft.touchpad_click_method = (*value).to_owned()
                    }
                    DropdownId::TouchpadTapMap => {
                        app.commons_draft.touchpad_tap_button_map = (*value).to_owned()
                    }
                    DropdownId::TouchpadAccel => {
                        app.commons_draft.touchpad_accel_profile = (*value).to_owned()
                    }
                    DropdownId::TouchpadScroll => {
                        app.commons_draft.touchpad_scroll_method = (*value).to_owned()
                    }
                    DropdownId::MouseAccel => {
                        app.commons_draft.mouse_accel_profile = (*value).to_owned()
                    }
                    DropdownId::MouseScroll => {
                        app.commons_draft.mouse_scroll_method = (*value).to_owned()
                    }
                    DropdownId::TrackpointAccel => {
                        app.commons_draft.trackpoint_accel_profile = (*value).to_owned()
                    }
                    _ => {}
                }
                return ChoiceTarget::Common;
            }
            Self::OverviewStyle(value) => app.commons_draft.overview_style = (*value).to_owned(),
            Self::HotCornerAction(corner, value) => match *corner {
                "top-left" => app.commons_draft.hot_corner_top_left_action = (*value).to_owned(),
                "top-right" => app.commons_draft.hot_corner_top_right_action = (*value).to_owned(),
                "bottom-left" => {
                    app.commons_draft.hot_corner_bottom_left_action = (*value).to_owned()
                }
                _ => app.commons_draft.hot_corner_bottom_right_action = (*value).to_owned(),
            },
            Self::Resolution(size) => {
                if let Some(page) = &mut app.display {
                    page.draft.custom_modeline = false;
                    page.draft.mode = size.map(|(width, height)| clarion_ipc::ConfiguredMode {
                        width,
                        height,
                        refresh: None,
                    });
                }
            }
            Self::Refresh(rate) => {
                if let Some(page) = &mut app.display {
                    page.draft.custom_modeline = false;
                    if let Some(mode) = page.draft.mode.as_mut() {
                        mode.refresh = *rate;
                    } else if let Some(rate) = rate {
                        let dimensions = page
                            .outputs
                            .iter()
                            .find(|output| Some(&output.name) == page.selected.as_ref())
                            .and_then(|output| output.current_mode)
                            .and_then(|index| {
                                page.outputs
                                    .iter()
                                    .find(|output| Some(&output.name) == page.selected.as_ref())
                                    .and_then(|output| output.modes.get(index))
                            })
                            .map(|mode| (mode.width, mode.height));
                        if let Some((width, height)) = dimensions {
                            page.draft.mode = Some(clarion_ipc::ConfiguredMode {
                                width,
                                height,
                                refresh: Some(*rate),
                            });
                        }
                    }
                }
            }
            Self::DisplayScale(scale) => {
                if let Some(page) = &mut app.display {
                    page.draft.scale = *scale;
                }
            }
            Self::Transform(key) => {
                if let Some(page) = &mut app.display {
                    page.draft.transform = transform_from_key(key);
                }
            }
            Self::Vrr(key) => {
                if let Some(page) = &mut app.display {
                    page.draft.vrr = vrr_from_key(key);
                }
            }
            Self::MaxBpc(value) => {
                if let Some(page) = &mut app.display {
                    page.draft.max_bpc = *value;
                }
            }
        }
        if matches!(
            self,
            Self::Size(_) | Self::Position(_) | Self::Launcher(_) | Self::Effect(_)
        ) {
            ChoiceTarget::Shell
        } else if matches!(
            self,
            Self::Resolution(_)
                | Self::Refresh(_)
                | Self::DisplayScale(_)
                | Self::Transform(_)
                | Self::Vrr(_)
                | Self::MaxBpc(_)
        ) {
            ChoiceTarget::Display
        } else {
            ChoiceTarget::Common
        }
    }
}

impl SettingsApp {
    fn dropdown_choices(&self, id: DropdownId) -> Vec<(DropdownChoice, SharedString)> {
        match id {
            DropdownId::DockSize => vec![
                (DropdownChoice::Size(32.), "小".into()),
                (DropdownChoice::Size(48.), "中".into()),
                (DropdownChoice::Size(64.), "大".into()),
            ],
            DropdownId::DockPosition => vec![
                (DropdownChoice::Position(DockPosition::Left), "左侧".into()),
                (DropdownChoice::Position(DockPosition::Bottom), "底部".into()),
                (DropdownChoice::Position(DockPosition::Right), "右侧".into()),
            ],
            DropdownId::Launcher => vec![
                (
                    DropdownChoice::Launcher(LauncherPresentation::Fullscreen),
                    "全屏".into(),
                ),
                (
                    DropdownChoice::Launcher(LauncherPresentation::Palette),
                    "弹窗".into(),
                ),
            ],
            DropdownId::MinimizeEffect => vec![
                (DropdownChoice::Effect(MinimizeEffect::Genie), "精灵".into()),
                (DropdownChoice::Effect(MinimizeEffect::Scale), "缩放".into()),
            ],
            DropdownId::CenterColumn => vec![
                (DropdownChoice::Center("never"), "从不".into()),
                (DropdownChoice::Center("on-overflow"), "超出屏幕时".into()),
                (DropdownChoice::Center("always"), "始终".into()),
            ],
            DropdownId::DefaultWidth => vec![
                (DropdownChoice::Width(None), "应用建议".into()),
                (
                    DropdownChoice::Width(Some(PresetSize::Proportion(0.25))),
                    "四分之一".into(),
                ),
                (
                    DropdownChoice::Width(Some(PresetSize::Proportion(1. / 3.))),
                    "三分之一".into(),
                ),
                (
                    DropdownChoice::Width(Some(PresetSize::Proportion(0.5))),
                    "二分之一".into(),
                ),
                (
                    DropdownChoice::Width(Some(PresetSize::Proportion(2. / 3.))),
                    "三分之二".into(),
                ),
                (
                    DropdownChoice::Width(Some(PresetSize::Proportion(0.75))),
                    "四分之三".into(),
                ),
                (
                    DropdownChoice::Width(Some(PresetSize::Proportion(1.))),
                    "整个可用宽度".into(),
                ),
            ],
            DropdownId::OpenEffect => vec![
                (DropdownChoice::OpenEffect("fade"), "淡入".into()),
                (DropdownChoice::OpenEffect("reveal"), "逐渐展开".into()),
            ],
            DropdownId::CloseEffect => vec![
                (DropdownChoice::CloseEffect("fade"), "淡出".into()),
                (DropdownChoice::CloseEffect("implode"), "收拢消失".into()),
                (DropdownChoice::CloseEffect("explosion"), "碎片消散".into()),
            ],
            DropdownId::IdleAction => vec![
                (DropdownChoice::IdleAction("none"), "不执行".into()),
                (DropdownChoice::IdleAction("lock"), "锁定".into()),
                (DropdownChoice::IdleAction("suspend"), "挂起".into()),
            ],
            DropdownId::CursorHide => vec![
                (DropdownChoice::CursorHide(None), "从不".into()),
                (DropdownChoice::CursorHide(Some(15_000)), "15 秒".into()),
                (DropdownChoice::CursorHide(Some(30_000)), "30 秒".into()),
                (DropdownChoice::CursorHide(Some(60_000)), "1 分钟".into()),
                (DropdownChoice::CursorHide(Some(120_000)), "2 分钟".into()),
                (DropdownChoice::CursorHide(Some(300_000)), "5 分钟".into()),
                (DropdownChoice::CursorHide(Some(600_000)), "10 分钟".into()),
                (DropdownChoice::CursorHide(Some(1_800_000)), "30 分钟".into()),
            ],
            DropdownId::GradientRelative => vec![
                (DropdownChoice::GradientRelative("window"), "窗口".into()),
                (
                    DropdownChoice::GradientRelative("workspace-view"),
                    "工作区视图".into(),
                ),
            ],
            DropdownId::TouchpadClick => vec![
                (DropdownChoice::CommonText(DropdownId::TouchpadClick, ""), "默认".into()),
                (
                    DropdownChoice::CommonText(DropdownId::TouchpadClick, "clickfinger"),
                    "两指点击".into(),
                ),
                (
                    DropdownChoice::CommonText(DropdownId::TouchpadClick, "button-areas"),
                    "按键区域".into(),
                ),
            ],
            DropdownId::TouchpadTapMap => vec![
                (
                    DropdownChoice::CommonText(DropdownId::TouchpadTapMap, ""),
                    "默认".into(),
                ),
                (
                    DropdownChoice::CommonText(DropdownId::TouchpadTapMap, "left-right-middle"),
                    "左 · 右 · 中".into(),
                ),
                (
                    DropdownChoice::CommonText(DropdownId::TouchpadTapMap, "left-middle-right"),
                    "左 · 中 · 右".into(),
                ),
            ],
            DropdownId::TouchpadAccel
            | DropdownId::MouseAccel
            | DropdownId::TrackpointAccel => {
                let id = match id {
                    DropdownId::MouseAccel => DropdownId::MouseAccel,
                    DropdownId::TrackpointAccel => DropdownId::TrackpointAccel,
                    _ => DropdownId::TouchpadAccel,
                };
                vec![
                    (DropdownChoice::CommonText(id, ""), "默认".into()),
                    (DropdownChoice::CommonText(id, "adaptive"), "自适应".into()),
                    (DropdownChoice::CommonText(id, "flat"), "线性".into()),
                ]
            }
            DropdownId::TouchpadScroll | DropdownId::MouseScroll => {
                let id = if id == DropdownId::MouseScroll {
                    DropdownId::MouseScroll
                } else {
                    DropdownId::TouchpadScroll
                };
                vec![
                    (DropdownChoice::CommonText(id, ""), "默认".into()),
                    (DropdownChoice::CommonText(id, "two-finger"), "双指".into()),
                    (DropdownChoice::CommonText(id, "edge"), "边缘".into()),
                    (
                        DropdownChoice::CommonText(id, "on-button-down"),
                        "按住按键".into(),
                    ),
                    (DropdownChoice::CommonText(id, "no-scroll"), "禁用".into()),
                ]
            }
            DropdownId::SystemTheme => self
                .system_themes
                .iter()
                .map(|family| {
                    (
                        DropdownChoice::SystemTheme(family.name.clone()),
                        SharedString::from(family.name.clone()),
                    )
                })
                .collect(),
            DropdownId::IconTheme => self
                .icon_themes
                .iter()
                .map(|family| {
                    (
                        DropdownChoice::IconTheme(family.name.clone()),
                        SharedString::from(family.name.clone()),
                    )
                })
                .collect(),
            DropdownId::CursorTheme => self
                .cursor_themes
                .iter()
                .map(|family| {
                    (
                        DropdownChoice::CursorTheme(family.name.clone()),
                        SharedString::from(family.name.clone()),
                    )
                })
                .collect(),
            DropdownId::OverviewStyle => vec![
                (DropdownChoice::OverviewStyle("stage"), "空间舞台".into()),
                (DropdownChoice::OverviewStyle("strip"), "经典纵向".into()),
            ],
            DropdownId::HotCornerTopLeft => hot_corner_action_choices("top-left"),
            DropdownId::HotCornerTopRight => hot_corner_action_choices("top-right"),
            DropdownId::HotCornerBottomLeft => hot_corner_action_choices("bottom-left"),
            DropdownId::HotCornerBottomRight => hot_corner_action_choices("bottom-right"),
            DropdownId::DisplayMaxBpc => {
                let cap = self
                    .display
                    .as_ref()
                    .and_then(|page| {
                        page.outputs
                            .iter()
                            .find(|output| Some(&output.name) == page.selected.as_ref())
                    })
                    .and_then(|output| output.max_bpc)
                    .unwrap_or(16);
                let mut choices: Vec<(DropdownChoice, SharedString)> =
                    vec![(DropdownChoice::MaxBpc(None), "自动".into())];
                for value in [6u8, 8, 10, 12, 14, 16] {
                    if value <= cap {
                        choices.push((
                            DropdownChoice::MaxBpc(Some(value)),
                            SharedString::from(format!("{value} 位")),
                        ));
                    }
                }
                choices
            }
            DropdownId::DisplayResolution => {
                let Some(page) = &self.display else {
                    return Vec::new();
                };
                let Some(output) = page
                    .outputs
                    .iter()
                    .find(|output| Some(&output.name) == page.selected.as_ref())
                else {
                    return Vec::new();
                };
                let mut sizes: Vec<(u16, u16)> = output
                    .modes
                    .iter()
                    .map(|mode| (mode.width, mode.height))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                sizes.sort_by(|left, right| right.cmp(left));
                let mut choices: Vec<(DropdownChoice, SharedString)> =
                    vec![(DropdownChoice::Resolution(None), "自动（推荐）".into())];
                for (width, height) in sizes {
                    let preferred = output
                        .modes
                        .iter()
                        .any(|mode| mode.width == width && mode.height == height && mode.is_preferred);
                    choices.push((
                        DropdownChoice::Resolution(Some((width, height))),
                        SharedString::from(format!(
                            "{width} × {height}{}",
                            if preferred { "（推荐）" } else { "" }
                        )),
                    ));
                }
                choices
            }
            DropdownId::DisplayRefresh => {
                let Some(page) = &self.display else {
                    return Vec::new();
                };
                let Some(output) = page
                    .outputs
                    .iter()
                    .find(|output| Some(&output.name) == page.selected.as_ref())
                else {
                    return Vec::new();
                };
                let dimensions = page
                    .draft
                    .mode
                    .map(|mode| (mode.width, mode.height))
                    .or_else(|| {
                        output
                            .current_mode
                            .and_then(|index| output.modes.get(index))
                            .map(|mode| (mode.width, mode.height))
                    });
                let rates: BTreeSet<u32> = output
                    .modes
                    .iter()
                    .filter(|mode| Some((mode.width, mode.height)) == dimensions)
                    .map(|mode| mode.refresh_rate)
                    .collect();
                let mut choices: Vec<(DropdownChoice, SharedString)> =
                    vec![(DropdownChoice::Refresh(None), "自动（推荐）".into())];
                for rate in rates {
                    choices.push((
                        DropdownChoice::Refresh(Some(f64::from(rate) / 1000.)),
                        SharedString::from(format!("{:.2} Hz", f64::from(rate) / 1000.)),
                    ));
                }
                choices
            }
            DropdownId::DisplayScale => {
                let mut choices: Vec<(DropdownChoice, SharedString)> =
                    vec![(DropdownChoice::DisplayScale(None), "自动".into())];
                for value in [1., 1.25, 1.5, 1.75, 2., 2.25, 2.5, 3., 4.] {
                    choices.push((
                        DropdownChoice::DisplayScale(Some(value)),
                        SharedString::from(format!("{:.0}%", value * 100.)),
                    ));
                }
                choices
            }
            DropdownId::DisplayTransform => vec![
                (DropdownChoice::Transform("normal"), "标准".into()),
                (DropdownChoice::Transform("90"), "向左旋转 90°".into()),
                (DropdownChoice::Transform("180"), "倒转 180°".into()),
                (DropdownChoice::Transform("270"), "向右旋转 90°".into()),
                (DropdownChoice::Transform("flipped"), "水平镜像".into()),
                (DropdownChoice::Transform("flipped-90"), "左转 90° / 镜像".into()),
                (DropdownChoice::Transform("flipped-180"), "垂直镜像".into()),
                (DropdownChoice::Transform("flipped-270"), "右转 90° / 镜像".into()),
            ],
            DropdownId::DisplayVrr => vec![
                (DropdownChoice::Vrr("off"), "关闭".into()),
                (DropdownChoice::Vrr("always"), "始终开启".into()),
                (DropdownChoice::Vrr("on-demand"), "按需".into()),
            ],
        }
    }
}

#[derive(Clone)]
struct EditorApp {
    name: String,
    exec: std::path::PathBuf,
}

#[derive(Clone)]
struct EditChooser {
    path: std::path::PathBuf,
    apps: Vec<EditorApp>,
}

#[derive(Clone)]
struct ArrangementDrag {
    name: String,
    start: Point<Pixels>,
    positions: Vec<(String, (i32, i32))>,
    scale: f64,
    offset: (f32, f32),
}

#[derive(Clone)]
struct DisplayPage {
    outputs: Vec<clarion_ipc::Output>,
    selected: Option<String>,
    target: OutputName,
    baseline: OutputSettings,
    draft: OutputSettings,
}

pub struct SettingsApp {
    pane: Pane,
    prefs: Preferences,
    committed: Preferences,
    bridge: ShellBridge,
    environment: HashMap<String, String>,
    facts: crate::system_info::SystemFacts,
    text_inputs: HashMap<TextKey, Entity<TextInput>>,
    color_picker: Option<ColorPickerState>,
    color_drag: Option<ColorDrag>,
    preset_selection: Option<PresetSelection>,
    preset_scroll: ScrollHandle,
    help_hover: Option<(&'static str, &'static str, Point<Pixels>)>,
    wallpaper_current: Option<PathBuf>,
    wallpaper_history: Vec<PathBuf>,
    wallpaper_thumbs: HashMap<PathBuf, WallpaperThumb>,
    wallpaper_loading: bool,
    system_theme: String,
    icon_theme: String,
    system_theme_family: String,
    icon_theme_family: String,
    cursor_theme_family: String,
    system_themes: Vec<ThemeFamily>,
    icon_themes: Vec<ThemeFamily>,
    cursor_themes: Vec<ThemeFamily>,
    status: Option<(String, bool, Instant)>,
    query: String,
    search: Entity<TextInput>,
    nav_scroll: ScrollHandle,
    detail_scroll: ScrollHandle,
    focus: FocusHandle,
    /// Bounds of each dropdown trigger, recorded while painting so the popup
    /// anchors to the visible circle rather than to the pointer.
    trigger_bounds: Rc<RefCell<HashMap<&'static str, Bounds<Pixels>>>>,
    /// Compositor configuration shown by the window/layout/input pages.
    commons: CommonSettings,
    commons_draft: CommonSettings,
    commons_error: Option<String>,
    commons_saving: bool,
    display: Option<DisplayPage>,
    display_loading: bool,
    display_preview: Option<ConfigManager>,
    display_busy: bool,
    display_cancelling: bool,
    display_restart: bool,
    display_message: Option<(String, bool)>,
    edit_chooser: Option<EditChooser>,
    arrangement_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
    arrangement_positions: Option<Vec<(String, (i32, i32))>>,
    arrangement_drag: Option<ArrangementDrag>,
    pending_positions: Option<Vec<(OutputName, (i32, i32))>>,
    menu_window: Option<WindowHandle<MenuView>>,
    menu_id: Option<DropdownId>,
    back: Vec<Pane>,
    forward: Vec<Pane>,
}

impl SettingsApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let environment: HashMap<String, String> = std::env::vars().collect();
        Self::with_environment(cx, environment, ShellBridge::from_environment())
    }

    fn with_environment(
        cx: &mut Context<Self>,
        environment: HashMap<String, String>,
        bridge: ShellBridge,
    ) -> Self {
        let (prefs, load_error) = match Preferences::load(&environment, false) {
            Ok(prefs) => (prefs, None),
            Err(error) => (
                Preferences::default(),
                Some(format!("读取桌面偏好失败：{error:#}")),
            ),
        };
        let dark = prefs.theme != "light";
        let search = cx.new(|cx| {
            TextInput::new(cx)
                .with_capsule(dark)
                .with_placeholder("搜索")
                .with_font_size(19.)
        });
        // Compositor-owned settings live in config.kdl; load a snapshot so the
        // window/layout/input pages start from what is really running.
        let facts = crate::system_info::SystemFacts::collect(&environment);
        let (commons, commons_error) = match ConfigManager::new()
            .and_then(|manager| CommonSettings::read(&manager))
        {
            Ok(commons) => (commons, None),
            Err(error) => (
                CommonSettings::default(),
                Some(format!("读取桌面配置失败：{error:#}")),
            ),
        };
        let commons_draft = commons.clone();
        let mut text_inputs = HashMap::new();
        for key in TextKey::ALL {
            let value = key.value(&commons_draft);
            let input = cx.new(|cx| {
                TextInput::new(cx)
                    .with_capsule(dark)
                    .with_font_size(13.)
                    .with_placeholder(key.placeholder())
            });
            input.update(cx, |input, cx| input.set_text(&value, false, cx));
            text_inputs.insert(key, input);
        }
        let (wallpaper_current, wallpaper_history) = wallpaper_state(&environment);
        let system_theme = read_gtk_setting(&environment, "gtk-theme-name").unwrap_or_default();
        let icon_theme =
            read_gtk_setting(&environment, "gtk-icon-theme-name").unwrap_or_default();
        let system_themes = theme_families(
            installed_system_themes(&environment, &system_theme),
            true,
        );
        let icon_themes =
            theme_families(installed_icon_themes(&environment, &icon_theme), true);
        let cursor_themes = theme_families(
            installed_cursor_themes(&environment, &commons.cursor_theme),
            false,
        );
        let system_theme_family = family_for(&system_theme, &system_themes);
        let icon_theme_family = family_for(&icon_theme, &icon_themes);
        let cursor_theme_family = family_for(&commons.cursor_theme, &cursor_themes);
        let app = Self {
            pane: Pane::Overview,
            committed: prefs.clone(),
            prefs,
            bridge,
            environment,
            facts,
            text_inputs,
            color_picker: None,
            color_drag: None,
            preset_selection: None,
            preset_scroll: ScrollHandle::new(),
            help_hover: None,
            wallpaper_current,
            wallpaper_history,
            wallpaper_thumbs: HashMap::new(),
            wallpaper_loading: false,
            system_theme,
            icon_theme,
            system_theme_family,
            icon_theme_family,
            cursor_theme_family,
            system_themes,
            icon_themes,
            cursor_themes,
            status: load_error.map(|message| (message, true, Instant::now())),
            query: String::new(),
            search: search.clone(),
            nav_scroll: ScrollHandle::new(),
            detail_scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            trigger_bounds: Rc::default(),
            commons,
            commons_draft: commons_draft.clone(),
            commons_error: commons_error.clone(),
            commons_saving: false,
            display: None,
            display_loading: false,
            display_preview: None,
            display_busy: false,
            display_cancelling: false,
            display_restart: false,
            display_message: None,
            edit_chooser: None,
            arrangement_bounds: Rc::default(),
            arrangement_positions: None,
            arrangement_drag: None,
            pending_positions: None,
            menu_window: None,
            menu_id: None,
            back: Vec::new(),
            forward: Vec::new(),
        };
        for (key, input) in app.text_inputs.clone() {
            cx.subscribe(&input, move |this, _, QueryChanged(text), cx| {
                this.text_changed(key, text.clone(), cx)
            })
            .detach();
        }
        cx.subscribe(&search, |this, _, QueryChanged(query), cx| {
            this.query = query.clone();
            cx.notify();
        })
        .detach();
        // A Shell Spawn can map the window without activation. Ask the
        // compositor to focus this PID's window once it exists.
        #[cfg(not(test))]
        cx.spawn(async move |_this, cx| {
            for _ in 0..60 {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let focused = cx
                    .background_executor()
                    .spawn(async { crate::compositor::focus_own_window().unwrap_or(false) })
                    .await;
                if focused {
                    break;
                }
            }
        })
        .detach();
        // Keep the sheet in step with Shell-side changes (theme, Dock pinning,
        // another settings window) without fighting an in-flight local edit.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_secs(2))
                .await;
            if this.update(cx, |this, cx| this.sync(cx)).is_err() {
                break;
            }
        })
        .detach();
        app
    }

    pub fn watch_signals(
        &mut self,
        window: &gpui::Window,
        signals: std::sync::mpsc::Receiver<crate::instance::Signal>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            while let Ok(signal) = signals.try_recv() {
                let _ = this.update(cx, |app, cx| {
                    if let crate::instance::Signal::Pane(name) = &signal {
                        if let Some(pane) = nav::from_arg(name) {
                            app.select(pane, cx);
                        }
                    }
                    cx.notify();
                });
                let _ = cx.update(|window, _| window.activate_window());
                crate::compositor::focus_own_window().ok();
            }
        })
        .detach();
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        if self.prefs != self.committed {
            if self.bridge.connected() {
                self.apply(cx);
            }
            return;
        }
        let Ok(next) = Preferences::load(&self.environment, false) else {
            return;
        };
        if next != self.prefs {
            self.prefs = next.clone();
            self.committed = next;
            self.search.update(cx, |input, cx| {
                input.set_dark(self.prefs.theme != "light", cx)
            });
            self.set_text_inputs_dark(cx);
            cx.notify();
        }
    }

    pub fn select(&mut self, pane: Pane, cx: &mut Context<Self>) {
        if pane == self.pane {
            return;
        }
        self.back.push(self.pane);
        if self.back.len() > 32 {
            self.back.remove(0);
        }
        self.forward.clear();
        self.pane = pane;
        self.load_displays_if_needed(cx);
        if pane == Pane::Appearance {
            self.refresh_wallpaper_state(cx);
        }
        cx.notify();
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.back.pop() {
            self.forward.push(self.pane);
            self.pane = pane;
            self.load_displays_if_needed(cx);
            if pane == Pane::Appearance {
                self.refresh_wallpaper_state(cx);
            }
            cx.notify();
        }
    }

    fn forward(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.forward.pop() {
            self.back.push(self.pane);
            self.pane = pane;
            self.load_displays_if_needed(cx);
            cx.notify();
        }
    }

    fn apply(&mut self, cx: &mut Context<Self>) {
        if self.prefs == self.committed {
            return;
        }
        let patch = match self.committed.diff(&self.prefs) {
            Ok(patch) => patch,
            Err(error) => {
                self.status = Some((format!("无法生成修改：{error:#}"), true, Instant::now()));
                return;
            }
        };
        match self.bridge.patch(&patch) {
            Ok(()) => {
                self.committed = self.prefs.clone();
                self.status = Some(("已应用".into(), false, Instant::now()));
                self.search.update(cx, |input, cx| {
                    input.set_dark(self.prefs.theme != "light", cx)
                });
                self.set_text_inputs_dark(cx);
            }
            Err(error) => {
                self.status = Some((format!("桌面未应用修改：{error:#}"), true, Instant::now()));
            }
        }
        cx.notify();
    }
}

fn apply_common_settings(
    before: &CommonSettings,
    after: &CommonSettings,
) -> anyhow::Result<()> {
    let mut manager = ConfigManager::new()?;
    CommonSettings::apply(&mut manager, before, after)?;
    let ipc = Connection::new();
    manager.save_automatic_with(|action| ipc.perform_action(action))
}

fn change(
    entity: &Entity<SettingsApp>,
    cx: &mut App,
    update: impl FnOnce(&mut Preferences),
) {
    entity.update(cx, |this, cx| {
        update(&mut this.prefs);
        this.apply(cx);
    });
}

impl Render for SettingsApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(self.prefs.theme != "light", &self.prefs.accent);
        let entity = cx.entity();
        let nav_items = nav::matching(&self.query);
        let searched = !self.query.trim().is_empty();

        let mut nav_list = div().flex().flex_col().gap(px(1.)).px(px(10.)).py(px(4.));
        let mut first_group = true;
        for (group, heading) in nav::GROUP_TITLES.iter().enumerate() {
            let items: Vec<&NavItem> = nav_items
                .iter()
                .copied()
                .filter(|item| item.group == group)
                .collect();
            if items.is_empty() || (searched && heading.is_empty()) {
                continue;
            }
            if !first_group {
                nav_list = nav_list.child(div().h(px(8.)));
            }
            first_group = false;
            if !heading.is_empty() {
                nav_list = nav_list.child(widgets::group_heading(heading, p, cx));
            }
            for item in items {
                let selected = item.pane == self.pane;
                let id = SharedString::from(format!("nav-{}", item.title));
                let target = item.pane;
                let select_entity = entity.clone();
                let search = self.search.clone();
                nav_list = nav_list.child(widgets::sidebar_row(
                    id,
                    item,
                    selected,
                    p,
                    cx,
                    window.scale_factor(),
                    move |_, cx| {
                        select_entity.update(cx, |this, cx| this.select(target, cx));
                        search.update(cx, |input, cx| input.set_text("", false, cx));
                    },
                ));
            }
        }
        if searched && nav_items.is_empty() {
            nav_list = nav_list.child(
                div().p(px(8.)).child(widgets::label(
                    "没有匹配的设置",
                    12.,
                    FontWeight::NORMAL,
                    p.muted,
                    cx,
                )),
            );
        }

        let sidebar = div()
            .w(px(theme::SIDEBAR_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .rounded(px(theme::PANEL_RADIUS))
            .overflow_hidden()
            .border_1()
            .border_color(p.panel_border)
            .bg(p.chrome.opacity(0.10))
            .shadow(vec![BoxShadow {
                color: p.shadow.opacity(0.55),
                offset: point(px(0.), px(4.)),
                blur_radius: px(18.),
                spread_radius: px(0.),
            }])
            .child(
                div()
                    .h(px(70.))
                    .flex_none()
                    .flex()
                    .relative()
                    .top(px(-6.))
                    .items_center()
                    .gap(px(4.))
                    .px(px(16.))
                    .child(traffic_light("traffic-close", rgb(0xff5f57).into(), |window| {
                        window.remove_window()
                    }))
                    .child(traffic_light("traffic-minimize", rgb(0xfebc2e).into(), |window| {
                        window.minimize_window()
                    }))
                    .child(traffic_light("traffic-zoom", rgb(0x28c840).into(), |window| {
                        window.zoom_window()
                    })),
            )
            .child(
                div()
                    .px(px(16.))
                    .pb(px(14.))
                    .child(
                        div()
                            .h(px(42.))
                            .rounded_full()
                            .border_1()
                            .border_color(p.search_border)
                            .bg(p.search_bg)
                            .shadow(vec![BoxShadow {
                                color: p.shadow.opacity(0.5),
                                offset: point(px(0.), px(1.)),
                                blur_radius: px(3.),
                                spread_radius: px(0.),
                            }])
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px(px(8.))
                            .child(widgets::magnifier(p, 20.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(self.search.clone()),
                            ),
                    ),
            )
            .child(
                div()
                    .id("settings-nav-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .scrollbar_width(px(3.))
                    .track_scroll(&self.nav_scroll)
                    .child(nav_list),
            )
            .child(div().h(px(10.)));

        let scrolled = self.detail_scroll.offset().y < px(-0.5);
        let title = nav::item(self.pane).title;
        let can_back = !self.back.is_empty();
        let can_forward = !self.forward.is_empty();
        let header_entity = entity.clone();
        let header = div()
            .absolute()
            .top(px(0.))
            .left(px(0.))
            .right(px(0.))
            .h(px(theme::HEADER_HEIGHT))
            .flex()
            .items_center()
            .gap(px(18.))
            .px(px(30.))
            .border_b_1()
            .border_color(p.separator.opacity(if scrolled { 0.9 } else { 0.4 }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .h(px(46.))
                    .px(px(3.))
                    .rounded_full()
                    .bg(p.control)
                    .border_1()
                    .border_color(p.control_border)
                    .shadow(vec![BoxShadow {
                        color: p.shadow.opacity(0.5),
                        offset: point(px(0.), px(1.)),
                        blur_radius: px(6.),
                        spread_radius: px(0.),
                    }])
                    .child(header_button(
                        "settings-back",
                        false,
                        can_back,
                        p,
                        cx,
                        {
                            let entity = header_entity.clone();
                            move |cx| {
                                entity.update(cx, |this, cx| this.back(cx));
                            }
                        },
                    ))
                    .child(
                        div()
                            .w(px(1.))
                            .h(px(22.))
                            .bg(p.separator.opacity(0.8)),
                    )
                    .child(header_button(
                        "settings-forward",
                        true,
                        can_forward,
                        p,
                        cx,
                        {
                            let entity = header_entity.clone();
                            move |cx| {
                                entity.update(cx, |this, cx| this.forward(cx));
                            }
                        },
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
                    .child(widgets::label(title, 18., FontWeight::SEMIBOLD, p.text, cx)),
            );

        let preview_panel = (self.pane == Pane::WindowEffects)
            .then(|| self.window_preview_panel(p, cx));
        let preview_height = if preview_panel.is_some() {
            WINDOW_PREVIEW_HEIGHT
        } else {
            0.
        };
        let page = self.page(&entity, p, cx);
        let status = self.status.as_ref().and_then(|(message, error, time)| {
            (time.elapsed() < Duration::from_secs(4)).then_some((message.clone(), *error))
        });

        let detail = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .rounded(px(theme::PANEL_RADIUS))
            .overflow_hidden()
            // A light wash on top of the translucent window base; the cards
            // and the header provide the contrast.
            .bg(p.chrome.opacity(0.10))
            .child(
                div()
                    .id("settings-detail-scroll")
                    .debug_selector(|| "settings-detail-scroll".into())
                    .absolute()
                    .top(px(theme::HEADER_HEIGHT + preview_height))
                    .left(px(0.))
                    .right(px(0.))
                    .bottom(px(0.))
                    .overflow_y_scroll()
                    .scrollbar_width(px(3.))
                    .track_scroll(&self.detail_scroll)
                    .child(div().w_full().pt(px(36.)).child(page)),
            )
            .when_some(preview_panel, |detail, preview| {
                detail.child(
                    div()
                        .absolute()
                        .top(px(theme::HEADER_HEIGHT))
                        .left(px(0.))
                        .right(px(0.))
                        .h(px(preview_height))
                        .bg(p.chrome.opacity(0.28))
                        .border_b_1()
                        .border_color(p.separator)
                        .child(preview),
                )
            })
            .child(
                div()
                    .absolute()
                    .top(px(theme::HEADER_HEIGHT + preview_height))
                    .left(px(0.))
                    .right(px(0.))
                    .h(px(8.))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(
                            p.shadow.opacity(if scrolled { 0.22 } else { 0.08 }),
                            0.,
                        ),
                        linear_color_stop(Hsla::transparent_black(), 1.),
                    )),
            )
            .child(header)
            .when_some(status, |el, (message, error)| {
                el.child(
                    div()
                        .absolute()
                        .bottom(px(0.))
                        .left(px(0.))
                        .right(px(0.))
                        .h(px(40.))
                        .px(px(30.))
                        .flex()
                        .items_center()
                        .rounded_b(px(theme::PANEL_RADIUS))
                        .bg(linear_gradient(
                            0.,
                            linear_color_stop(p.chrome.opacity(0.98), 0.),
                            linear_color_stop(p.chrome_alt.opacity(0.92), 1.),
                        ))
                        .child(widgets::label(
                            message,
                            11.,
                            FontWeight::NORMAL,
                            if error { rgb(0xff453a).into() } else { p.muted },
                            cx,
                        )),
                )
            });

        let display_modal = if self.display_preview.is_some() {
            let seconds = self
                .display_preview
                .as_ref()
                .and_then(|manager| manager.preview_seconds());
            let (message, error) = self
                .display_message
                .as_ref()
                .map(|(message, error)| (message.clone(), *error))
                .unwrap_or_else(|| ("正在准备显示预览…".into(), false));
            let confirm = entity.clone();
            let restore = entity.clone();
            Some(
                div()
                    .id("display-modal-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00000055))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("display-modal-card")
                            .debug_selector(|| "display-modal-card".into())
                            .w(px(460.))
                            .rounded(px(18.))
                            .border_1()
                            .border_color(p.control_border)
                            .bg(p.control)
                            .shadow(vec![BoxShadow {
                                color: p.shadow.opacity(0.7),
                                offset: point(px(0.), px(10.)),
                                blur_radius: px(28.),
                                spread_radius: px(0.),
                            }])
                            .p(px(22.))
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(widgets::label(
                                "确认显示设置",
                                16.,
                                FontWeight::SEMIBOLD,
                                p.text,
                                cx,
                            ))
                            .child(widgets::label(
                                message,
                                12.,
                                FontWeight::NORMAL,
                                if error { rgb(0xff453a).into() } else { p.muted },
                                cx,
                            ))
                            .when_some(seconds, |el, seconds| {
                                el.child(widgets::label(
                                    format!("将在 {seconds} 秒后自动恢复"),
                                    11.,
                                    FontWeight::NORMAL,
                                    p.muted,
                                    cx,
                                ))
                            })
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(8.))
                                    .child(widgets::button(
                                        "display-modal-restore",
                                        "恢复",
                                        p,
                                        cx,
                                        move |_, cx| {
                                            restore.update(cx, |this, cx| {
                                                this.cancel_display_preview(cx)
                                            });
                                        },
                                    ))
                                    .child(widgets::button(
                                        "display-modal-confirm",
                                        "保留并保存",
                                        p,
                                        cx,
                                        move |_, cx| {
                                            confirm.update(cx, |this, cx| {
                                                this.confirm_display_preview(cx)
                                            });
                                        },
                                    )),
                            ),
                    ),
            )
        } else {
            None
        };

        let edit_chooser = self.edit_chooser.as_ref().map(|chooser| {
            let path = chooser.path.clone();
            let mut rows = Vec::new();
            for (index, app) in chooser.apps.iter().take(6).enumerate() {
                let entity = entity.clone();
                let exec = app.exec.clone();
                let target = path.clone();
                let app_name = app.name.clone();
                let binary = app
                    .exec
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                rows.push(
                    div()
                        .id(SharedString::from(format!("edit-app-{index}")))
                        .debug_selector(move || format!("edit-app-{index}"))
                        .h(px(38.))
                        .px(px(12.))
                        .rounded(px(8.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(10.))
                        .cursor_pointer()
                        .hover(move |div| div.bg(p.hover))
                        .child(widgets::label(
                            app_name,
                            12.5,
                            FontWeight::NORMAL,
                            p.text,
                            cx,
                        ))
                        .child(widgets::label(
                            binary,
                            10.5,
                            FontWeight::NORMAL,
                            p.muted,
                            cx,
                        ))
                        .on_click(move |_, _, cx| {
                            entity.update(cx, |this, cx| {
                                this.launch_editor(exec.clone(), target.clone(), cx)
                            });
                        })
                        .into_any_element(),
                );
            }
            let default_entity = entity.clone();
            let default_path = path.clone();
            let default_row = div()
                .id("edit-app-default")
                .debug_selector(|| "edit-app-default".into())
                .h(px(38.))
                .px(px(12.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.))
                .cursor_pointer()
                .hover(move |div| div.bg(p.hover))
                .child(widgets::label(
                    "系统默认应用",
                    12.5,
                    FontWeight::NORMAL,
                    p.text,
                    cx,
                ))
                .on_click(move |_, _, cx| {
                    default_entity.update(cx, |this, cx| {
                        this.edit_chooser = None;
                        this.open_file(default_path.clone(), cx);
                    });
                });
            let cancel = entity.clone();
            div()
                    .id("edit-chooser-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00000055))
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.close_edit_chooser(cx);
                    }))
                    .child(
                        div()
                            .id("edit-chooser-card")
                            .debug_selector(|| "edit-chooser-card".into())
                            .w(px(440.))
                            .rounded(px(18.))
                            .border_1()
                            .border_color(p.control_border)
                            .bg(p.control)
                            .shadow(vec![BoxShadow {
                                color: p.shadow.opacity(0.7),
                                offset: point(px(0.), px(10.)),
                                blur_radius: px(28.),
                                spread_radius: px(0.),
                            }])
                            .p(px(18.))
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(widgets::label(
                                "打开方式",
                                16.,
                                FontWeight::SEMIBOLD,
                                p.text,
                                cx,
                            ))
                            .child(widgets::label(
                                short_path(&path.display().to_string()),
                                11.,
                                FontWeight::NORMAL,
                                p.muted,
                                cx,
                            ))
                            .children(rows)
                            .child(default_row)
                            .child(
                                div().flex().justify_end().pt(px(4.)).child(
                                    widgets::button(
                                        "edit-chooser-cancel",
                                        "取消",
                                        p,
                                        cx,
                                        move |_, cx| {
                                            cancel.update(cx, |this, cx| {
                                                this.close_edit_chooser(cx)
                                            });
                                        },
                                    ),
                                ),
                            ),
                    )
        });

        let color_picker = self.color_picker.as_ref().map(|state| {
            const SV_W: f32 = 300.;
            const SV_H: f32 = 170.;
            const BAR_W: f32 = 230.;
            let bounds = self.trigger_bounds.clone();
            let sv_bounds = bounds.clone();
            let hue_bounds = bounds.clone();
            let alpha_bounds = bounds.clone();
            let done = entity.clone();
            let cancel = entity.clone();
            let clear = entity.clone();
            let accent_only = state.target == ColorPickerTarget::Accent;
            let optional = state.target.allows_empty();
            let hue_color = hsla_from_hsv(state.hue, 1., 1., 1.);
            let preview = hsla_from_hsv(state.hue, state.saturation, state.value, state.alpha);
            let hex = color_hex(state.hue, state.saturation, state.value, state.alpha);
            let (hue, saturation, value, alpha) =
                (state.hue, state.saturation, state.value, state.alpha);

            div()
                .id("color-picker-backdrop")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000055))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.close_color_picker(cx)
                }))
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                    this.update_color_drag(event.position, cx)
                }))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.color_drag = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("color-picker-card")
                        .debug_selector(|| "color-picker-card".into())
                        .w(px(340.))
                        .p(px(18.))
                        .rounded(px(18.))
                        .border_1()
                        .border_color(p.control_border)
                        .bg(p.control)
                        .shadow(vec![BoxShadow {
                            color: p.shadow.opacity(0.7),
                            offset: point(px(0.), px(10.)),
                            blur_radius: px(28.),
                            spread_radius: px(0.),
                        }])
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(widgets::label(
                                    "选择颜色",
                                    16.,
                                    FontWeight::SEMIBOLD,
                                    p.text,
                                    cx,
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(
                                            div()
                                                .size(px(24.))
                                                .rounded_full()
                                                .bg(preview)
                                                .border_1()
                                                .border_color(p.control_border),
                                        )
                                        .child(widgets::label(
                                            hex,
                                            10.5,
                                            FontWeight::NORMAL,
                                            p.muted,
                                            cx,
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .id("color-sv")
                                .debug_selector(|| "color-sv".into())
                                .relative()
                                .w(px(SV_W))
                                .h(px(SV_H))
                                .rounded(px(8.))
                                .overflow_hidden()
                                .bg(hue_color)
                                .child(
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .bg(linear_gradient(
                                            90.,
                                            linear_color_stop(rgba(0xffffffff), 0.),
                                            linear_color_stop(rgba(0xffffff00), 1.),
                                        )),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .bg(linear_gradient(
                                            180.,
                                            linear_color_stop(rgba(0x00000000), 0.),
                                            linear_color_stop(rgba(0x000000ff), 1.),
                                        )),
                                )
                                .child(
                                    gpui::canvas(
                                        move |bounds, _, _| {
                                            sv_bounds.borrow_mut().insert("color-sv", bounds);
                                        },
                                        |_, _, _, _| {},
                                    )
                                    .absolute()
                                    .inset_0(),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(saturation * SV_W - 7.))
                                        .top(px((1. - value) * SV_H - 7.))
                                        .size(px(14.))
                                        .rounded_full()
                                        .border_2()
                                        .border_color(rgb(0xffffff)),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                                        this.set_color_drag(ColorDrag::Sv, event.position, cx)
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .child(widgets::label(
                                    "色相",
                                    11.,
                                    FontWeight::NORMAL,
                                    p.muted,
                                    cx,
                                ))
                                .child(
                                    div()
                                        .id("color-hue")
                                        .debug_selector(|| "color-hue".into())
                                        .relative()
                                        .w(px(BAR_W))
                                        .h(px(16.))
                                        .rounded(px(4.))
                                        .overflow_hidden()
                                        .child(
                                            gpui::canvas(
                                                move |bounds, _, _| {
                                                    hue_bounds
                                                        .borrow_mut()
                                                        .insert("color-hue", bounds);
                                                },
                                                move |bounds, _, window, _| {
                                                    let width = f32::from(bounds.size.width);
                                                    for index in 0..60 {
                                                        let start = bounds.left()
                                                            + px(width * index as f32 / 60.);
                                                        let end = bounds.left()
                                                            + px(width * (index + 1) as f32 / 60.);
                                                        window.paint_quad(fill(
                                                            Bounds::new(
                                                                point(start, bounds.top()),
                                                                size(
                                                                    (end - start).max(px(1.)),
                                                                    bounds.size.height,
                                                                ),
                                                            ),
                                                            hsla_from_hsv(
                                                                index as f32 / 60.,
                                                                1.,
                                                                1.,
                                                                1.,
                                                            ),
                                                        ));
                                                    }
                                                },
                                            )
                                            .size_full(),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .left(px(hue * BAR_W - 2.))
                                                .top(px(0.))
                                                .w(px(4.))
                                                .h(px(16.))
                                                .bg(rgb(0xffffff))
                                                .border_1()
                                                .border_color(rgba(0x00000055)),
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                                                this.set_color_drag(ColorDrag::Hue, event.position, cx)
                                            }),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .when(accent_only, |row| row.hidden())
                                .child(widgets::label(
                                    "透明度",
                                    11.,
                                    FontWeight::NORMAL,
                                    p.muted,
                                    cx,
                                ))
                                .child(
                                    div()
                                        .id("color-alpha")
                                        .debug_selector(|| "color-alpha".into())
                                        .relative()
                                        .w(px(BAR_W))
                                        .h(px(16.))
                                        .rounded(px(4.))
                                        .overflow_hidden()
                                        .child(
                                            gpui::canvas(
                                                move |bounds, _, _| {
                                                    alpha_bounds
                                                        .borrow_mut()
                                                        .insert("color-alpha", bounds);
                                                },
                                                move |bounds, _, window, _| {
                                                    let width = f32::from(bounds.size.width);
                                                    for row in 0..2 {
                                                        for column in 0..(width / 8.).ceil() as usize {
                                                            let color = if (row + column) % 2 == 0 {
                                                                p.card
                                                            } else {
                                                                p.text.opacity(0.10)
                                                            };
                                                            window.paint_quad(fill(
                                                                Bounds::new(
                                                                    point(
                                                                        bounds.left()
                                                                            + px(column as f32 * 8.),
                                                                        bounds.top()
                                                                            + px(row as f32 * 8.),
                                                                    ),
                                                                    size(px(8.), px(8.)),
                                                                ),
                                                                color,
                                                            ));
                                                        }
                                                    }
                                                    for index in 0..60 {
                                                        let start = bounds.left()
                                                            + px(width * index as f32 / 60.);
                                                        let end = bounds.left()
                                                            + px(width * (index + 1) as f32 / 60.);
                                                        window.paint_quad(fill(
                                                            Bounds::new(
                                                                point(start, bounds.top()),
                                                                size(
                                                                    (end - start).max(px(1.)),
                                                                    bounds.size.height,
                                                                ),
                                                            ),
                                                            hsla_from_hsv(
                                                                hue,
                                                                saturation,
                                                                value,
                                                                index as f32 / 60.,
                                                            ),
                                                        ));
                                                    }
                                                },
                                            )
                                            .size_full(),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .left(px(alpha * BAR_W - 2.))
                                                .top(px(0.))
                                                .w(px(4.))
                                                .h(px(16.))
                                                .bg(rgb(0xffffff))
                                                .border_1()
                                                .border_color(rgba(0x00000055)),
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                                                this.set_color_drag(
                                                    ColorDrag::Alpha,
                                                    event.position,
                                                    cx,
                                                )
                                            }),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.))
                                .when(optional, |row| {
                                    row.child(widgets::button(
                                        "color-clear",
                                        "自动",
                                        p,
                                        cx,
                                        move |_, cx| {
                                            clear.update(cx, |this, cx| {
                                                this.clear_color_picker(cx)
                                            });
                                        },
                                    ))
                                })
                                .child(widgets::button(
                                    "color-cancel",
                                    "取消",
                                    p,
                                    cx,
                                    move |_, cx| {
                                        cancel.update(cx, |this, cx| this.close_color_picker(cx));
                                    },
                                ))
                                .child(widgets::button(
                                    "color-done",
                                    "完成",
                                    p,
                                    cx,
                                    move |_, cx| {
                                        done.update(cx, |this, cx| this.commit_color_picker(cx));
                                    },
                                )),
                        ),
                )
        });

        let mut root = div()
            .id("settings-root")
            .size_full()
            .relative()
            .flex()
            .gap(px(theme::GUTTER))
            .p(px(theme::GUTTER))
            .rounded(px(theme::WINDOW_RADIUS))
            .overflow_hidden()
            .border_1()
            .border_color(p.window_border)
            .bg(linear_gradient(
                150.,
                linear_color_stop(p.window, 0.),
                linear_color_stop(p.window_alt, 1.),
            ))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    if this.color_picker.is_some() {
                        this.close_color_picker(cx);
                    } else if this.edit_chooser.is_some() {
                        this.close_edit_chooser(cx);
                    } else if this.display_preview.is_some() {
                        this.cancel_display_preview(cx);
                    } else {
                        this.close_menu(cx);
                    }
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
            }))
            .child(sidebar)
            .child(detail);
        if let Some(modal) = display_modal {
            root = root.child(modal);
        }
        if let Some(chooser) = edit_chooser {
            root = root.child(chooser);
        }
        if let Some(picker) = color_picker {
            root = root.child(picker);
        }
        if let Some((_, text, position)) = self.help_hover {
            let viewport = window.viewport_size();
            let width = 300.;
            let x = (position.x + px(14.))
                .min(viewport.width - px(width + 10.))
                .max(px(8.));
            let y = (position.y + px(20.))
                .min(viewport.height - px(64.))
                .max(px(8.));
            root = root.child(
                div()
                    .id("help-tooltip")
                    .debug_selector(|| "help-tooltip".into())
                    .absolute()
                    .left(x)
                    .top(y)
                    .w(px(width))
                    .rounded(px(9.))
                    .border_1()
                    .border_color(p.panel_border)
                    .bg(p.chrome.opacity(0.98))
                    .shadow(vec![BoxShadow {
                        color: p.shadow.opacity(0.6),
                        offset: point(px(0.), px(4.)),
                        blur_radius: px(14.),
                        spread_radius: px(0.),
                    }])
                    .px(px(10.))
                    .py(px(8.))
                    .child(widgets::label(
                        text,
                        11.,
                        FontWeight::NORMAL,
                        p.text,
                        cx,
                    )),
            );
        }
        root
    }
}

#[allow(clippy::too_many_arguments)]
fn text_width(text: &str, size: f32) -> f32 {
    text.chars()
        .map(|character| {
            if character.is_ascii() {
                size * 0.56
            } else {
                size
            }
        })
        .sum()
}

#[allow(clippy::too_many_arguments)]
fn open_dropdown_menu(
    entity: &Entity<SettingsApp>,
    id: DropdownId,
    choices: Vec<(DropdownChoice, SharedString)>,
    selected: usize,
    p: Palette,
    trigger: Bounds<Pixels>,
    main_window: &Window,
    cx: &mut App,
) {
    let text_size = 14. * theme::TEXT_SCALE;
    let text_width = choices
        .iter()
        .map(|(_, title)| text_width(title.as_ref(), text_size))
        .fold(0., f32::max);
    let width = (54. + text_width).clamp(300., 420.);
    let desired_height = 12. + choices.len() as f32 * 46.;
    // `Window::bounds` keeps the bounds this client asked for; Wayland never
    // reports where the compositor really placed the toplevel. Ask the
    // compositor so the overlay can anchor to the actual button position.
    let placement = crate::compositor::own_window_placement()
        .ok()
        .flatten();
    let (window_origin, output_size) = match &placement {
        Some(placement) => (
            placement.position,
            (placement.output.width as f64, placement.output.height as f64),
        ),
        None => {
            let bounds = main_window.bounds();
            (
                (f64::from(bounds.origin.x), f64::from(bounds.origin.y)),
                (
                    f64::from(main_window.viewport_size().width),
                    f64::from(main_window.viewport_size().height),
                ),
            )
        }
    };
    let output_width = output_size.0 as f32;
    let output_height = output_size.1 as f32;
    let height = desired_height.min((output_height - 32.).max(200.));
    let anchor_x = window_origin.0 as f32 + f32::from(trigger.right());
    let anchor_y = window_origin.1 as f32 + f32::from(trigger.bottom());
    let trigger_top = window_origin.1 as f32 + f32::from(trigger.top());
    let x = ((anchor_x + 14.) - width).clamp(8., (output_width - width - 8.).max(8.));
    let y = if anchor_y + 4. + height + 8. > output_height {
        (trigger_top - height - 4.).max(8.)
    } else {
        anchor_y + 4.
    };
    let card = Bounds::new(point(px(x), px(y)), size(px(width), px(height)));
    let display_id = placement.as_ref().and_then(|placement| {
        let logical = &placement.output;
        cx.displays()
            .into_iter()
            .find(|display| {
                let bounds = display.bounds();
                (f32::from(bounds.origin.x) - logical.x as f32).abs() < 1.
                    && (f32::from(bounds.origin.y) - logical.y as f32).abs() < 1.
                    && (f32::from(bounds.size.width) - logical.width as f32).abs() < 1.
                    && (f32::from(bounds.size.height) - logical.height as f32).abs() < 1.
            })
            .map(|display| display.id())
    });

    // A card painted inside the settings window can never be frosted: the
    // compositor blurs what lies behind a surface, not the surface's own
    // pixels. A full-screen transparent overlay carries only the card and asks
    // for the rounded card outline as its glass region, exactly like the
    // shell's desktop menu and launcher menus.
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(0.), px(0.)),
            size(px(output_width), px(output_height)),
        ))),
        titlebar: None,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "clarion-settings-menu",
            layer: LayerShellLayer::Overlay,
            output: display_id,
            anchor: LayerShellAnchor::ALL,
            margins: LayerShellMargins::default(),
            exclusive_zone: -1,
            keyboard_interactivity: LayerShellKeyboardInteractivity::Exclusive,
            keep_alive: false,
        }),
        display_id,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        focus: true,
        show: true,
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some("clarion-settings-menu".into()),
        ..Default::default()
    };
    let app = entity.downgrade();
    let main_handle = main_window.window_handle();
    let handle = cx
        .open_window(options, move |window, cx| {
            window.set_window_title("设置选项");
            let view = cx.new(|cx| MenuView::new(app, choices, selected, p, main_handle, card, cx));
            let focus = view.read(cx).focus.clone();
            window.focus(&focus);
            window.activate_window();
            view
        })
        .ok();
    let opened = handle.is_some();
    entity.update(cx, |this, cx| {
        this.menu_window = handle;
        this.menu_id = opened.then_some(id);
        cx.notify();
    });
}

struct MenuView {
    app: WeakEntity<SettingsApp>,
    choices: Vec<(DropdownChoice, SharedString)>,
    selected: usize,
    p: Palette,
    main_window: AnyWindowHandle,
    card: Bounds<Pixels>,
    glass: bool,
    focus: FocusHandle,
}

impl MenuView {
    fn new(
        app: WeakEntity<SettingsApp>,
        choices: Vec<(DropdownChoice, SharedString)>,
        selected: usize,
        p: Palette,
        main_window: AnyWindowHandle,
        card: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            app,
            choices,
            selected,
            p,
            main_window,
            card,
            glass: false,
            focus: cx.focus_handle(),
        }
    }

    fn finish(&mut self, cx: &mut Context<Self>) {
        let _ = self.app.update(cx, |this, cx| {
            this.menu_window = None;
            this.menu_id = None;
            cx.notify();
        });
        let main = self.main_window;
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, _| window.activate_window());
        });
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish(cx);
        window.remove_window();
    }
}

impl Render for MenuView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.p;
        if !self.glass {
            // Set the outline before asking for the effect so the compositor
            // never blurs the whole output for one frame.
            let mut regions = rounded_region(
                f32::from(self.card.size.width),
                f32::from(self.card.size.height),
                14.,
            );
            for region in &mut regions {
                region.origin.x += self.card.origin.x;
                region.origin.y += self.card.origin.y;
            }
            window.set_glass_region(Some(&regions));
            window.set_background_appearance(WindowBackgroundAppearance::Blurred);
            self.glass = true;
        }
        let mut card = div()
            .id("settings-dropdown-menu")
            .debug_selector(|| "settings-dropdown-menu".into())
            .absolute()
            .left(self.card.origin.x)
            .top(self.card.origin.y)
            .w(self.card.size.width)
            .h(self.card.size.height)
            .rounded(px(14.))
            .border_1()
            .border_color(p.control_border)
            .bg(p.control)
            .shadow(vec![BoxShadow {
                color: p.shadow.opacity(0.65),
                offset: point(px(0.), px(6.)),
                blur_radius: px(20.),
                spread_radius: px(0.),
            }])
            .p(px(6.))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let mut list = div()
            .id("settings-dropdown-list")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .scrollbar_width(px(3.))
            .flex()
            .flex_col()
            .gap(px(2.));
        for (index, (choice, title)) in self.choices.iter().cloned().enumerate() {
            let selected = index == self.selected;
            let app = self.app.clone();
            let main = self.main_window;
            let color = if selected { p.on_accent } else { p.text };
            list = list.child(
                div()
                    .id(SharedString::from(format!("menu-option-{index}")))
                    .debug_selector(move || format!("menu-option-{index}"))
                    .h(px(46.))
                    .px(px(8.))
                    .rounded(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .when(selected, |div| div.bg(p.accent))
                    .when(!selected, |div| div.hover(move |div| div.bg(p.hover)))
                    .child(
                        div()
                            .w(px(18.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(widgets::label(
                                if selected { "✓" } else { "" },
                                14.,
                                FontWeight::MEDIUM,
                                color,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(widgets::label(
                                title.to_string(),
                                14.,
                                FontWeight::MEDIUM,
                                color,
                                cx,
                            )),
                    )
                    .on_click(move |_, window, cx| {
                        let _ = app.update(cx, |this, cx| {
                            let target = choice.apply(this);
                            this.menu_window = None;
                            this.menu_id = None;
                            match target {
                                ChoiceTarget::Shell => this.apply(cx),
                                ChoiceTarget::Common => this.queue_common(cx),
                                ChoiceTarget::Display => this.display_changed(cx),
                            }
                            cx.notify();
                        });
                        cx.defer(move |cx| {
                            let _ = main.update(cx, |_, window, _| window.activate_window());
                        });
                        window.remove_window();
                    }),
            );
        }
        card = card.child(list);
        div()
            .id("settings-menu-backdrop")
            .size_full()
            .relative()
            .bg(rgba(0x00000001))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.close(window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close(window, cx)),
            )
            .child(card)
    }
}

/// Rectangle mask for a rounded outline, in window-local pixels. Wayland's
/// background-effect region is a rectangle list, so each corner is
/// approximated with one-pixel bands.
fn rounded_region(width: f32, height: f32, radius: f32) -> Vec<Bounds<Pixels>> {
    if width <= 0. || height <= 0. {
        return vec![];
    }
    let r = radius.min(width / 2.).min(height / 2.).max(0.);
    let mut regions = vec![Bounds::new(
        point(px(0.), px(r)),
        size(px(width), px((height - 2. * r).max(0.))),
    )];
    for y in 0..r.ceil() as u32 {
        let d = r - y as f32 - 0.5;
        let inset = (r - (r * r - d * d).max(0.).sqrt()).ceil();
        let w = (width - 2. * inset).max(0.);
        for top in [y as f32, height - y as f32 - 1.] {
            regions.push(Bounds::new(point(px(inset), px(top)), size(px(w), px(1.))));
        }
    }
    regions
}

/// Positions the live outputs inside a fixed-size arrangement canvas. Values
/// are relative to the canvas; the drag math only needs the same scale.
/// Push a dragged output out of every other output. The smallest of the four
/// axis displacements is applied repeatedly so the preview never shows an
/// overlapping layout.
fn resolve_output_position(
    position: (i32, i32),
    size: (u32, u32),
    others: &[(i32, i32, u32, u32)],
) -> (i32, i32) {
    let (mut x, mut y) = position;
    let width = size.0 as i32;
    let height = size.1 as i32;
    if width == 0 || height == 0 {
        return position;
    }
    for _ in 0..8 {
        let mut changed = false;
        for (other_x, other_y, other_width, other_height) in others {
            let other_width = *other_width as i32;
            let other_height = *other_height as i32;
            if x < other_x + other_width
                && x + width > *other_x
                && y < other_y + other_height
                && y + height > *other_y
            {
                let left = other_x - width;
                let right = other_x + other_width;
                let up = other_y - height;
                let down = other_y + other_height;
                let distances = [
                    ((left - x).abs(), 0u8),
                    ((right - x).abs(), 1),
                    ((up - y).abs(), 2),
                    ((down - y).abs(), 3),
                ];
                let (_, direction) = distances
                    .iter()
                    .min_by_key(|(distance, _)| *distance)
                    .copied()
                    .unwrap_or((0, 0));
                match direction {
                    0 => x = left,
                    1 => x = right,
                    2 => y = up,
                    _ => y = down,
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (x, y)
}

type ArrangementRects = Vec<(String, f32, f32, f32, f32)>;
type ArrangementLayout = (ArrangementRects, f64, f32, f32);

fn arrangement_layout(
    outputs: &[clarion_ipc::Output],
    positions: Option<&[(String, (i32, i32))]>,
    size: gpui::Size<Pixels>,
    fixed: Option<(f64, f32, f32)>,
) -> ArrangementLayout {
    let mut rects = Vec::new();
    for output in outputs {
        let Some(logical) = output.logical else {
            continue;
        };
        let (x, y) = positions
            .and_then(|positions| {
                positions
                    .iter()
                    .find(|(name, _)| name == &output.name)
                    .map(|(_, position)| *position)
            })
            .unwrap_or((logical.x, logical.y));
        rects.push((
            output.name.clone(),
            x as f64,
            y as f64,
            f64::from(logical.width),
            f64::from(logical.height),
        ));
    }
    if rects.is_empty() {
        return (Vec::new(), 0.03, 0., 0.);
    }
    let min_x = rects.iter().map(|rect| rect.1).fold(f64::INFINITY, f64::min);
    let min_y = rects.iter().map(|rect| rect.2).fold(f64::INFINITY, f64::min);
    let max_x = rects
        .iter()
        .map(|rect| rect.1 + rect.3)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = rects
        .iter()
        .map(|rect| rect.2 + rect.4)
        .fold(f64::NEG_INFINITY, f64::max);
    let width = f32::from(size.width).max(1.);
    let height = f32::from(size.height).max(1.);
    let (scale, offset_x, offset_y) = if let Some((scale, x, y)) = fixed {
        (scale, x as f64, y as f64)
    } else {
        let pad = 18.;
        let scale = ((width - pad * 2.) / (max_x - min_x) as f32)
            .min((height - pad * 2.) / (max_y - min_y) as f32)
            .clamp(0.02, 0.5) as f64;
        let layout_width = (max_x - min_x) * scale;
        let layout_height = (max_y - min_y) * scale;
        (
            scale,
            (width as f64 - layout_width) / 2. - min_x * scale,
            (height as f64 - layout_height) / 2. - min_y * scale,
        )
    };
    (
        rects
            .into_iter()
            .map(|(name, x, y, w, h)| {
                (
                    name,
                    (offset_x + x * scale) as f32,
                    (offset_y + y * scale) as f32,
                    (w * scale) as f32,
                    (h * scale) as f32,
                )
            })
            .collect(),
        scale,
        offset_x as f32,
        offset_y as f32,
    )
}

fn traffic_light(
    id: &'static str,
    color: Hsla,
    action: impl Fn(&mut Window) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(|| id.into())
        .size(px(26.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .hover(move |div| div.bg(rgba(0xffffff10)))
        .child(
            div()
                .size(px(18.))
                .rounded_full()
                .bg(color)
                .border_1()
                .border_color(rgba(0x00000024)),
        )
        .on_click(move |_, window, _| action(window))
}

fn header_button(
    id: &'static str,
    right: bool,
    enabled: bool,
    p: Palette,
    _cx: &gpui::App,
    action: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(|| id.into())
        .size(px(38.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(enabled, |div| div.cursor_pointer().hover(move |div| div.bg(p.hover)))
        .child(widgets::chevron(
            right,
            24.,
            if enabled { p.text } else { p.muted.opacity(0.4) },
        ))
        .on_click(move |_, _, cx| {
            if enabled {
                action(cx);
            }
        })
}

impl SettingsApp {
    fn current_dropdown_label(&self, id: DropdownId) -> String {
        match id {
            DropdownId::DockSize => match self.prefs.dock_size {
                32. => "小".into(),
                48. => "中".into(),
                64. => "大".into(),
                value => format!("{value:.0} pt"),
            },
            DropdownId::DockPosition => match self.prefs.dock_position {
                DockPosition::Left => "左侧".into(),
                DockPosition::Bottom => "底部".into(),
                DockPosition::Right => "右侧".into(),
            },
            DropdownId::Launcher => match self.prefs.launcher_presentation {
                LauncherPresentation::Fullscreen => "全屏".into(),
                LauncherPresentation::Palette => "弹窗".into(),
            },
            DropdownId::MinimizeEffect => match self.prefs.minimize_effect {
                MinimizeEffect::Genie => "精灵".into(),
                MinimizeEffect::Scale => "缩放".into(),
            },
            DropdownId::CenterColumn => match self.commons_draft.center_column.as_str() {
                "always" => "始终".into(),
                "on-overflow" => "超出屏幕时".into(),
                _ => "从不".into(),
            },
            DropdownId::DefaultWidth => match self.commons_draft.default_width {
                None => "应用建议".into(),
                Some(PresetSize::Proportion(value)) => {
                    let percent = value * 100.;
                    match percent.round() as i32 {
                        25 => "四分之一".into(),
                        33 => "三分之一".into(),
                        50 => "二分之一".into(),
                        67 => "三分之二".into(),
                        75 => "四分之三".into(),
                        100 => "整个可用宽度".into(),
                        _ => format!("自定义 · {percent:.1}%"),
                    }
                }
                Some(PresetSize::Fixed(value)) => format!("自定义 · {value} px"),
            },
            DropdownId::OpenEffect => match self.commons_draft.window_open_effect.as_str() {
                "reveal" => "逐渐展开".into(),
                _ => "淡入".into(),
            },
            DropdownId::CloseEffect => match self.commons_draft.window_close_effect.as_str() {
                "implode" => "收拢消失".into(),
                "explosion" => "碎片消散".into(),
                _ => "淡出".into(),
            },
            DropdownId::IdleAction => match self.commons_draft.idle_action.as_str() {
                "lock" => "锁定".into(),
                "suspend" => "挂起".into(),
                _ => "不执行".into(),
            },
            DropdownId::CursorHide => self
                .commons_draft
                .cursor_hide_after_ms
                .map(|ms| {
                    if ms < 60_000 {
                        format!("{} 秒", ms / 1000)
                    } else {
                        format!("{} 分钟", ms / 60_000)
                    }
                })
                .unwrap_or_else(|| "从不".into()),
            DropdownId::GradientRelative => {
                match self.commons_draft.border_gradient_relative.as_str() {
                    "workspace-view" => "工作区视图".into(),
                    _ => "窗口".into(),
                }
            }
            DropdownId::TouchpadClick => match self.commons_draft.touchpad_click_method.as_str() {
                "clickfinger" => "两指点击".into(),
                "button-areas" => "按键区域".into(),
                _ => "默认".into(),
            },
            DropdownId::TouchpadTapMap => {
                match self.commons_draft.touchpad_tap_button_map.as_str() {
                    "left-right-middle" => "左 · 右 · 中".into(),
                    "left-middle-right" => "左 · 中 · 右".into(),
                    _ => "默认".into(),
                }
            }
            DropdownId::TouchpadAccel => match self.commons_draft.touchpad_accel_profile.as_str() {
                "adaptive" => "自适应".into(),
                "flat" => "线性".into(),
                _ => "默认".into(),
            },
            DropdownId::MouseAccel => match self.commons_draft.mouse_accel_profile.as_str() {
                "adaptive" => "自适应".into(),
                "flat" => "线性".into(),
                _ => "默认".into(),
            },
            DropdownId::TrackpointAccel => {
                match self.commons_draft.trackpoint_accel_profile.as_str() {
                    "adaptive" => "自适应".into(),
                    "flat" => "线性".into(),
                    _ => "默认".into(),
                }
            }
            DropdownId::TouchpadScroll => {
                match self.commons_draft.touchpad_scroll_method.as_str() {
                    "two-finger" => "双指".into(),
                    "edge" => "边缘".into(),
                    "on-button-down" => "按住按键".into(),
                    "no-scroll" => "禁用".into(),
                    _ => "默认".into(),
                }
            }
            DropdownId::MouseScroll => match self.commons_draft.mouse_scroll_method.as_str() {
                "two-finger" => "双指".into(),
                "edge" => "边缘".into(),
                "on-button-down" => "按住按键".into(),
                "no-scroll" => "禁用".into(),
                _ => "默认".into(),
            },
            DropdownId::SystemTheme => {
                if self.system_theme_family.is_empty() {
                    "系统默认".into()
                } else {
                    self.system_theme_family.clone().into()
                }
            }
            DropdownId::IconTheme => {
                if self.icon_theme_family.is_empty() {
                    "系统默认".into()
                } else {
                    self.icon_theme_family.clone().into()
                }
            }
            DropdownId::CursorTheme => {
                if self.cursor_theme_family.is_empty() {
                    "系统默认".into()
                } else {
                    self.cursor_theme_family.clone().into()
                }
            }
            DropdownId::OverviewStyle => match self.commons_draft.overview_style.as_str() {
                "strip" => "经典纵向".into(),
                _ => "空间舞台".into(),
            },
            DropdownId::HotCornerTopLeft => {
                hot_corner_action_label(&self.commons_draft.hot_corner_top_left_action)
            }
            DropdownId::HotCornerTopRight => {
                hot_corner_action_label(&self.commons_draft.hot_corner_top_right_action)
            }
            DropdownId::HotCornerBottomLeft => {
                hot_corner_action_label(&self.commons_draft.hot_corner_bottom_left_action)
            }
            DropdownId::HotCornerBottomRight => {
                hot_corner_action_label(&self.commons_draft.hot_corner_bottom_right_action)
            }
            DropdownId::DisplayMaxBpc => self
                .display
                .as_ref()
                .and_then(|page| page.draft.max_bpc)
                .map(|value| format!("{value} 位"))
                .unwrap_or_else(|| "自动".into()),
            DropdownId::DisplayResolution => {
                let Some(page) = &self.display else {
                    return "自动（推荐）".into();
                };
                if page.draft.custom_modeline {
                    "自定义时序".into()
                } else {
                    page.draft
                        .mode
                        .map(|mode| format!("{} × {}", mode.width, mode.height))
                        .unwrap_or_else(|| "自动（推荐）".into())
                }
            }
            DropdownId::DisplayRefresh => {
                let Some(page) = &self.display else {
                    return "自动（推荐）".into();
                };
                if page.draft.custom_modeline {
                    "自定义时序".into()
                } else {
                    page.draft
                        .mode
                        .and_then(|mode| mode.refresh)
                        .map(|rate| format!("{rate:.2} Hz"))
                        .unwrap_or_else(|| "自动（推荐）".into())
                }
            }
            DropdownId::DisplayScale => self
                .display
                .as_ref()
                .and_then(|page| page.draft.scale)
                .map(|scale| format!("{:.0}%", scale * 100.))
                .unwrap_or_else(|| "自动".into()),
            DropdownId::DisplayTransform => {
                let key = self
                    .display
                    .as_ref()
                    .map(|page| transform_key(page.draft.transform))
                    .unwrap_or("normal");
                transform_label(key).into()
            }
            DropdownId::DisplayVrr => {
                let key = self
                    .display
                    .as_ref()
                    .map(|page| vrr_key(page.draft.vrr))
                    .unwrap_or("off");
                vrr_label(key).into()
            }
        }
    }

    fn dropdown_control(
        &self,
        entity: &Entity<SettingsApp>,
        element_id: &'static str,
        id: DropdownId,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.dropdown_control_state(entity, element_id, id, true, p, cx)
    }

    fn dropdown_control_state(
        &self,
        entity: &Entity<SettingsApp>,
        element_id: &'static str,
        id: DropdownId,
        enabled: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.current_dropdown_label(id);
        if !enabled {
            return div()
                .flex()
                .items_center()
                .gap(px(10.))
                .opacity(0.45)
                .child(widgets::label(selected, 14., FontWeight::MEDIUM, p.text, cx))
                .child(
                    div()
                        .size(px(34.))
                        .flex_none()
                        .rounded_full()
                        .bg(p.chip)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(widgets::chevron_up_down(p.muted, 16.)),
                )
                .into_any_element();
        }
        let entity = entity.clone();
        let click_choices = self.dropdown_choices(id);
        let trigger_bounds = self.trigger_bounds.clone();
        let click_bounds = trigger_bounds.clone();
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(widgets::label(
                selected,
                14.,
                FontWeight::MEDIUM,
                p.text,
                cx,
            ))
            .child(
                div()
                    .id(element_id)
                    .debug_selector(move || element_id.into())
                    .relative()
                    .size(px(34.))
                    .flex_none()
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .bg(p.chip)
                    .hover(move |div| div.bg(p.chip_hover))
                    .child(
                        gpui::canvas(
                            move |bounds, _, _| {
                                trigger_bounds.borrow_mut().insert(element_id, bounds);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .child(widgets::chevron_up_down(p.muted, 16.))
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        let choices = click_choices.clone();
                        let trigger = click_bounds
                            .borrow()
                            .get(element_id)
                            .copied()
                            .unwrap_or_else(|| {
                                Bounds::new(
                                    point(event.position.x - px(17.), event.position.y - px(17.)),
                                    size(px(34.), px(34.)),
                                )
                            });
                        let mut selected = 0;
                        let mut open = false;
                        let mut p = None;
                        entity.update(cx, |this, cx| {
                            let same = this.menu_id == Some(id) && this.menu_window.is_some();
                            this.close_menu(cx);
                            open = !same;
                            if open {
                                let selected_index = choices
                                    .iter()
                                    .position(|(choice, _)| choice.selected(this))
                                    .unwrap_or(usize::MAX);
                                selected = selected_index;
                                p = Some(theme::palette(
                                    this.prefs.theme != "light",
                                    &this.prefs.accent,
                                ));
                            }
                            if !open {
                                cx.notify();
                            }
                        });
                        if let (true, Some(p)) = (open, p) {
                            open_dropdown_menu(
                                &entity,
                                id,
                                choices,
                                selected,
                                p,
                                trigger,
                                window,
                                cx,
                            );
                        }
                        cx.stop_propagation();
                    }),
            )
            .into_any_element()
    }

    fn text_changed(&mut self, key: TextKey, text: String, cx: &mut Context<Self>) {
        let value = text.trim().to_owned();
        let changed = match key {
            TextKey::XkbLayout => {
                let changed = self.commons_draft.keyboard_xkb_layout != value;
                self.commons_draft.keyboard_xkb_layout = value;
                changed
            }
            TextKey::XkbVariant => {
                let changed = self.commons_draft.keyboard_xkb_variant != value;
                self.commons_draft.keyboard_xkb_variant = value;
                changed
            }
            TextKey::XkbOptions => {
                let changed = self.commons_draft.keyboard_xkb_options != value;
                self.commons_draft.keyboard_xkb_options = value;
                changed
            }
            TextKey::IdleLockCommand => {
                let changed = self.commons_draft.idle_lock_command != value;
                self.commons_draft.idle_lock_command = value;
                changed
            }
            TextKey::ScreenshotPath => {
                let changed = self.commons_draft.screenshot_path != value;
                self.commons_draft.screenshot_path = value;
                changed
            }
            TextKey::XwaylandPath => {
                let changed = self.commons_draft.xwayland_path != value;
                self.commons_draft.xwayland_path = value;
                changed
            }
        };
        if changed {
            self.queue_common(cx);
        }
    }

    fn queue_common(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) {
            // UI tests use a synthetic HOME, but ConfigManager resolves the
            // real user config. Never let a test rewrite the live config.
            cx.notify();
            return;
        }
        if self.commons_saving || self.commons == self.commons_draft {
            return;
        }
        self.commons_saving = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(350))
                .await;
            let Ok((before, after)) = this.update(cx, |this, _| {
                (this.commons.clone(), this.commons_draft.clone())
            }) else {
                return;
            };
            if before == after {
                let _ = this.update(cx, |this, cx| {
                    this.commons_saving = false;
                    cx.notify();
                });
                return;
            }
            let result = cx
                .background_executor()
                .spawn(async move { apply_common_settings(&before, &after) })
                .await;
            let _ = this.update(cx, |this, cx| this.finish_common(result, cx));
        })
        .detach();
    }

    fn finish_common(&mut self, result: anyhow::Result<()>, cx: &mut Context<Self>) {
        self.commons_saving = false;
        match result {
            Ok(()) => {
                self.commons = self.commons_draft.clone();
                self.commons_error = None;
                self.status = Some(("桌面设置已应用".into(), false, Instant::now()));
            }
            Err(error) => {
                self.status = Some((
                    format!("桌面设置未应用：{error:#}"),
                    true,
                    Instant::now(),
                ));
                // The file may have changed externally; show what is on disk.
                if let Ok(manager) = ConfigManager::new() {
                    if let Ok(fresh) = CommonSettings::read(&manager) {
                        self.commons = fresh.clone();
                        self.commons_draft = fresh;
                    }
                }
            }
        }
        if self.commons != self.commons_draft {
            self.queue_common(cx);
        }
        self.sync_text_inputs(cx);
        cx.notify();
    }

    fn sync_text_inputs(&self, cx: &mut Context<Self>) {
        for (key, input) in &self.text_inputs {
            let value = key.value(&self.commons_draft);
            input.update(cx, |input, cx| {
                if input.text() != value {
                    input.set_text(&value, false, cx);
                }
            });
        }
    }

    fn set_text_inputs_dark(&self, cx: &mut Context<Self>) {
        let dark = self.prefs.theme != "light";
        for input in self.text_inputs.values() {
            input.update(cx, |input, cx| input.set_dark(dark, cx));
        }
    }

    fn desktop_action(&mut self, action: clarion_ipc::Action, cx: &mut Context<Self>) {
        match Connection::new().perform_action(action) {
            Ok(()) => {
                self.status = Some(("操作已发送".into(), false, Instant::now()));
            }
            Err(error) => {
                self.status = Some((format!("操作失败：{error:#}"), true, Instant::now()));
            }
        }
        cx.notify();
    }

    fn config_file_path(&self) -> std::path::PathBuf {
        if let Some(path) = clarion_config::identity::config_path_from_environment() {
            return path;
        }
        let config_dir = self
            .environment
            .get("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| {
                self.environment
                    .get("HOME")
                    .map(|home| std::path::PathBuf::from(home).join(".config"))
            })
            .unwrap_or_else(|| std::path::PathBuf::from(".config"));
        clarion_config::identity::user_config_path(&config_dir)
    }

    fn open_editor_chooser(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let apps = discover_editors();
        if apps.is_empty() {
            self.open_file(path, cx);
            return;
        }
        self.edit_chooser = Some(EditChooser { path, apps });
        cx.notify();
    }

    fn launch_editor(
        &mut self,
        exec: std::path::PathBuf,
        path: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.edit_chooser = None;
        match std::process::Command::new(&exec).arg(&path).spawn() {
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                self.status = Some((
                    format!(
                        "已用 {} 打开 {}",
                        exec.file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "编辑器".into()),
                        short_path(&path.display().to_string())
                    ),
                    false,
                    Instant::now(),
                ));
            }
            Err(error) => {
                self.status = Some((
                    format!("无法启动编辑器：{error}"),
                    true,
                    Instant::now(),
                ));
            }
        }
        cx.notify();
    }

    fn close_edit_chooser(&mut self, cx: &mut Context<Self>) {
        if self.edit_chooser.take().is_some() {
            cx.notify();
        }
    }

    fn open_file(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let spawned = std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .or_else(|_| {
                std::process::Command::new("gio")
                    .arg("open")
                    .arg(&path)
                    .spawn()
            });
        match spawned {
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                self.status = Some((
                    format!(
                        "已请求系统编辑器打开 {}",
                        short_path(&path.display().to_string())
                    ),
                    false,
                    Instant::now(),
                ));
            }
            Err(error) => {
                self.status = Some((
                    format!("无法打开文件：{error}"),
                    true,
                    Instant::now(),
                ));
            }
        }
        cx.notify();
    }

    fn load_displays_if_needed(&mut self, cx: &mut Context<Self>) {
        #[cfg(not(test))]
        if self.pane == Pane::Displays && self.display.is_none() && !self.display_loading {
            self.refresh_displays(cx);
        }
        #[cfg(test)]
        let _ = cx;
    }

    fn refresh_displays(&mut self, cx: &mut Context<Self>) {
        if self.display_loading || self.display_preview.is_some() || self.display_busy {
            return;
        }
        self.display_loading = true;
        let previous = self.display.as_ref().and_then(|page| page.selected.clone());
        cx.spawn(async move |this, cx| {
            let data = cx
                .background_executor()
                .spawn(async move {
                    let connection = Connection::new();
                    let outputs = connection.get_outputs()?;
                    let selected = previous
                        .filter(|name| outputs.iter().any(|output| &output.name == name))
                        .or_else(|| {
                            outputs
                                .iter()
                                .find(|output| output.logical.is_some())
                                .map(|output| output.name.clone())
                        })
                        .or_else(|| outputs.first().map(|output| output.name.clone()));
                    let Some(name) = selected else {
                        return Ok::<_, anyhow::Error>((outputs, None, None));
                    };
                    let output = outputs
                        .iter()
                        .find(|output| output.name == name)
                        .expect("selected output exists");
                    let target = OutputName::from_ipc_output(output);
                    let manager = ConfigManager::new()?;
                    let settings = OutputSettings::read(&manager, &target)?;
                    Ok((outputs, Some(name), Some((target, settings))))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.display_loading = false;
                match data {
                    Ok((outputs, Some(selected), Some((target, settings)))) => {
                        this.arrangement_positions = None;
                        this.display = Some(DisplayPage {
                            outputs,
                            selected: Some(selected),
                            target,
                            baseline: settings.clone(),
                            draft: settings,
                        });
                    }
                    Ok(_) => {
                        this.display = None;
                        this.display_message =
                            Some(("当前没有可用的显示器。".into(), false));
                    }
                    Err(error) => {
                        this.display_message =
                            Some((format!("读取显示器配置失败：{error:#}"), true));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn display_changed(&mut self, cx: &mut Context<Self>) {
        if self.display_preview.is_some() {
            self.display_restart = true;
            if !self.display_cancelling {
                self.display_cancelling = true;
                let ipc = Connection::new();
                if let Some(manager) = &mut self.display_preview {
                    if let Err(error) = manager.finish_preview(&ipc, false) {
                        self.display_message = Some((format!("{error:#}"), true));
                    }
                }
            }
            return;
        }
        if self.display_busy {
            self.display_restart = true;
            return;
        }
        self.start_display_preview(cx);
    }

    fn start_display_preview(&mut self, cx: &mut Context<Self>) {
        if self.display_preview.is_some() || self.display_busy {
            return;
        }
        let Some(page) = &self.display else {
            return;
        };
        if page.draft == page.baseline && self.pending_positions.is_none() {
            return;
        }
        self.display_busy = true;
        self.display_message = Some(("正在准备预览…".into(), false));
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(350))
                .await;
            let Ok(snapshot) = this.update(cx, |this, _| {
                this.display.as_ref().map(|page| {
                    (
                        page.target.clone(),
                        page.baseline.clone(),
                        page.draft.clone(),
                        this.pending_positions.clone(),
                    )
                })
            }) else {
                return;
            };
            let Some((target, before, after, positions)) = snapshot else {
                return;
            };
            if before == after && positions.is_none() {
                let _ = this.update(cx, |this, cx| {
                    this.display_busy = false;
                    cx.notify();
                });
                return;
            }
            let previewed = after.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut manager = ConfigManager::new()?;
                    if before != after {
                        OutputSettings::apply(&mut manager, &target, &before, &after)?;
                    }
                    if let Some(positions) = &positions {
                        OutputSettings::apply_positions(&mut manager, positions)?;
                    }
                    let ipc = Connection::new();
                    manager.begin_output_preview(&ipc)?;
                    Ok::<ConfigManager, anyhow::Error>(manager)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.display_busy = false;
                match result {
                    Ok(manager) => {
                        this.display_preview = Some(manager);
                        this.pending_positions = None;
                        this.display_restart = this
                            .display
                            .as_ref()
                            .is_some_and(|page| page.draft != previewed);
                        this.display_cancelling = false;
                        this.display_message =
                            Some(("显示设置正在预览，请在倒计时结束前确认。".into(), false));
                        this.start_display_poll(cx);
                    }
                    Err(error) => {
                        this.display_message =
                            Some((format!("无法预览显示设置：{error:#}"), true));
                        this.display_restart = false;
                        this.pending_positions = None;
                        this.arrangement_positions = None;
                        if let Some(page) = &mut this.display {
                            page.draft = page.baseline.clone();
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_display_poll(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let keep = this
                .update(cx, |this, cx| this.poll_display(cx))
                .unwrap_or(false);
            if !keep {
                break;
            }
        })
        .detach();
    }

    fn poll_display(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(manager) = self.display_preview.as_mut() else {
            return false;
        };
        let ipc = Connection::new();
        match manager.poll_preview(&ipc) {
            Some(Ok(message)) => {
                if manager.preview_pending() {
                    self.display_message = Some((message, false));
                    cx.notify();
                    return true;
                }
                let saved = message.contains("已确认并保存");
                self.finish_display_preview(saved, None, cx);
                false
            }
            Some(Err(error)) => {
                self.finish_display_preview(false, Some(format!("{error:#}")), cx);
                false
            }
            None => {
                self.display_message = Some((
                    manager
                        .preview_seconds()
                        .map(|seconds| format!("显示设置正在预览，{seconds} 秒内确认。"))
                        .unwrap_or_else(|| "正在等待 Clarion 预览…".into()),
                    false,
                ));
                cx.notify();
                true
            }
        }
    }

    fn finish_display_preview(
        &mut self,
        saved: bool,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.display_preview = None;
        self.display_busy = false;
        self.display_cancelling = false;
        self.pending_positions = None;
        if !self.display_restart {
            self.arrangement_positions = None;
        }
        let restart = std::mem::take(&mut self.display_restart);
        if let Some(error) = error {
            self.display_message = Some((error, true));
            if let Some(page) = &mut self.display {
                page.draft = page.baseline.clone();
            }
            self.refresh_displays(cx);
            return;
        }
        if saved {
            if let Some(page) = &mut self.display {
                page.baseline = page.draft.clone();
            }
            self.display_message = Some(("显示设置已保存".into(), false));
            self.refresh_displays(cx);
            return;
        }
        if restart {
            self.display_message = Some(("正在预览最新调整…".into(), false));
            self.start_display_preview(cx);
            cx.notify();
            return;
        }
        if let Some(page) = &mut self.display {
            page.draft = page.baseline.clone();
        }
        self.display_message = Some(("已恢复原显示设置".into(), false));
        self.refresh_displays(cx);
    }

    fn begin_arrangement_drag(
        &mut self,
        name: String,
        start: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.display_preview.is_some() || self.display_busy {
            return;
        }
        let Some(page) = &self.display else {
            return;
        };
        let mut positions = Vec::new();
        for output in &page.outputs {
            if let Some(logical) = output.logical {
                positions.push((output.name.clone(), (logical.x, logical.y)));
            }
        }
        if !positions.iter().any(|(output, _)| output == &name) {
            return;
        }
        let outputs = page.outputs.clone();
        if let Some(page) = &mut self.display {
            page.selected = Some(name.clone());
        }
        // Recompute the transform from the live layout instead of trusting a
        // possibly stale canvas size from an earlier frame; otherwise a tiny
        // bounds value turns a short drag into a huge logical jump.
        let canvas_size = self
            .arrangement_bounds
            .borrow()
            .map(|bounds| bounds.size)
            .unwrap_or_else(|| size(px(758.), px(250.)));
        let (_, scale, offset_x, offset_y) =
            arrangement_layout(&outputs, None, canvas_size, None);
        self.arrangement_positions = Some(positions.clone());
        self.arrangement_drag = Some(ArrangementDrag {
            name,
            start,
            positions,
            scale,
            offset: (offset_x, offset_y),
        });
        cx.notify();
    }

    fn update_arrangement_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = &self.arrangement_drag else {
            return;
        };
        let Some(page) = &self.display else {
            return;
        };
        let dx = (f32::from(position.x - drag.start.x) / drag.scale as f32).round() as i32;
        let dy = (f32::from(position.y - drag.start.y) / drag.scale as f32).round() as i32;
        let mut positions = drag.positions.clone();
        let Some((_, start)) = positions
            .iter()
            .find(|(output, _)| output == &drag.name)
            .map(|(name, position)| (name.clone(), *position))
        else {
            return;
        };
        let mut size = (0, 0);
        let mut others = Vec::new();
        for output in &page.outputs {
            let Some(logical) = output.logical else {
                continue;
            };
            if output.name == drag.name {
                size = (logical.width, logical.height);
                continue;
            }
            if let Some((_, (x, y))) = positions.iter().find(|(name, _)| name == &output.name) {
                others.push((*x, *y, logical.width, logical.height));
            }
        }
        let (x, y) = resolve_output_position(
            (start.0 + dx, start.1 + dy),
            size,
            &others,
        );
        if let Some((_, output)) = positions
            .iter_mut()
            .find(|(output, _)| output == &drag.name)
        {
            *output = (x, y);
        }
        self.arrangement_positions = Some(positions);
        cx.notify();
    }

    fn finish_arrangement_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.arrangement_drag.take() else {
            return;
        };
        let Some(positions) = self.arrangement_positions.clone() else {
            return;
        };
        if positions == drag.positions {
            self.arrangement_positions = None;
            self.refresh_displays(cx);
            return;
        }
        self.commit_arrangement(positions, cx);
    }

    fn commit_arrangement(
        &mut self,
        positions: Vec<(String, (i32, i32))>,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = &self.display else {
            return;
        };
        let mut request = Vec::new();
        let mut moved = false;
        for output in &page.outputs {
            let Some(logical) = output.logical else {
                continue;
            };
            let Some(position) = positions
                .iter()
                .find(|(name, _)| name == &output.name)
                .map(|(_, position)| *position)
            else {
                continue;
            };
            if position != (logical.x, logical.y) {
                moved = true;
            }
            request.push((OutputName::from_ipc_output(output), position));
        }
        if !moved || request.is_empty() {
            self.arrangement_positions = None;
            return;
        }
        self.pending_positions = Some(request);
        self.start_display_preview(cx);
    }

    fn confirm_display_preview(&mut self, cx: &mut Context<Self>) {
        if self.display_busy {
            return;
        }
        let ipc = Connection::new();
        if let Some(manager) = &mut self.display_preview {
            match manager.finish_preview(&ipc, true) {
                Ok(()) => {
                    self.display_busy = true;
                    self.display_message = Some(("正在保存显示设置…".into(), false));
                }
                Err(error) => {
                    self.display_message = Some((format!("{error:#}"), true));
                }
            }
        }
        cx.notify();
    }

    fn cancel_display_preview(&mut self, cx: &mut Context<Self>) {
        let ipc = Connection::new();
        if let Some(manager) = &mut self.display_preview {
            if let Err(error) = manager.finish_preview(&ipc, false) {
                self.display_message = Some((format!("{error:#}"), true));
            }
        }
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.menu_window.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        self.menu_id = None;
    }

    fn page(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let content = match self.pane {
            Pane::Overview => self.overview(entity, p, cx),
            Pane::Appearance => self.appearance(entity, p, cx),
            Pane::Dock => self.dock(entity, p, cx),
            Pane::MenuBar => self.menu_bar(entity, p, cx),
            Pane::Accessibility => self.accessibility(entity, p, cx),
            Pane::WindowEffects => self.window_effects(entity, p, cx),
            Pane::Layout => self.layout_page(entity, p, cx),
            Pane::Keyboard => self.keyboard_page(entity, p, cx),
            Pane::Trackpad => self.trackpad_page(entity, p, cx),
            Pane::System => self.system_page(entity, p, cx),
            Pane::MissionControl => self.mission_control(entity, p, cx),
            Pane::Displays => self.displays_page(entity, p, cx),
            Pane::Power => self.power_page(entity, p, cx),
        };
        div()
            .w_full()
            .px(px(30.))
            .pb(px(30.))
            .flex()
            .flex_col()
            .child(content)
            .into_any_element()
    }

    fn overview(
        &mut self,
        _entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let preferences_path =
            clarion_preferences::preferences_path(&self.environment).ok();
        let config_path = self.config_file_path();
        let theme = if self.prefs.theme == "light" {
            "浅色"
        } else {
            "深色"
        };
        let position = match self.prefs.dock_position {
            DockPosition::Left => "左侧",
            DockPosition::Bottom => "底部",
            DockPosition::Right => "右侧",
        };
        let connected = if self.bridge.connected() {
            "已连接"
        } else {
            "未连接"
        };
        let facts = &self.facts;

        let mut about = Vec::new();
        if !facts.model.is_empty() {
            about.push(
                widgets::value_row("型号", facts.model.clone(), p, cx).into_any_element(),
            );
        }
        if !facts.hostname.is_empty() && facts.hostname != facts.model {
            about.push(
                widgets::value_row("设备名称", facts.hostname.clone(), p, cx)
                    .into_any_element(),
            );
        }
        if !facts.cpu.is_empty() {
            about.push(
                widgets::value_row("芯片", facts.cpu.clone(), p, cx).into_any_element(),
            );
        }
        if !facts.gpu.is_empty() {
            about.push(
                widgets::value_row("显卡", facts.gpu.clone(), p, cx).into_any_element(),
            );
        }
        if let Some(bytes) = facts.memory_total {
            about.push(
                widgets::value_row(
                    "内存",
                    crate::system_info::format_gib(bytes),
                    p,
                    cx,
                )
                .into_any_element(),
            );
        }
        let disk = match (facts.disk_free, facts.disk_total) {
            (Some(free), Some(total)) => format!(
                "{} 可用 / {}",
                crate::system_info::format_gib(free),
                crate::system_info::format_gib(total)
            ),
            (None, Some(total)) => crate::system_info::format_gib(total),
            _ => String::new(),
        };
        if !disk.is_empty() {
            about.push(
                widgets::value_row("启动磁盘", disk, p, cx).into_any_element(),
            );
        }
        if !facts.serial.is_empty() {
            about.push(
                widgets::value_row("序列号", facts.serial.clone(), p, cx)
                    .into_any_element(),
            );
        }
        if !facts.os_name.is_empty() {
            about.push(
                widgets::value_row("系统", facts.os_name.clone(), p, cx).into_any_element(),
            );
        }
        if !facts.kernel.is_empty() {
            about.push(
                widgets::value_row("内核", facts.kernel.clone(), p, cx)
                    .into_any_element(),
            );
        }
        if about.is_empty() {
            about.push(
                widgets::label(
                    "无法读取设备信息。",
                    11.,
                    FontWeight::NORMAL,
                    p.muted,
                    cx,
                )
                .into_any_element(),
            );
        }

        let preferences_label = preferences_path
            .as_ref()
            .map(|path| short_path(&path.display().to_string()))
            .unwrap_or_else(|| "未知".into());
        let config_label = short_path(&config_path.display().to_string());

        let preferences_control = {
            let open = cx.entity();
            let path = preferences_path.clone();
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(widgets::label(
                    preferences_label,
                    11.,
                    FontWeight::NORMAL,
                    p.muted,
                    cx,
                ))
                .when_some(path, |el, path| {
                    el.child(widgets::button(
                        "edit-preferences",
                        "编辑",
                        p,
                        cx,
                        move |_, cx| {
                            open.update(cx, |this, cx| {
                                this.open_editor_chooser(path.clone(), cx)
                            });
                        },
                    ))
                })
        };
        let config_control = {
            let open = cx.entity();
            let path = config_path.clone();
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(widgets::label(
                    config_label,
                    11.,
                    FontWeight::NORMAL,
                    p.muted,
                    cx,
                ))
                .child(widgets::button("edit-config", "编辑", p, cx, move |_, cx| {
                    open.update(cx, |this, cx| {
                        this.open_editor_chooser(path.clone(), cx)
                    });
                }))
        };

        div()
            .flex()
            .flex_col()
            .child(widgets::section(
                "关于本机",
                p,
                cx,
                widgets::stack(about, p),
            ))
            .child(widgets::section(
                "系统",
                p,
                cx,
                widgets::stack(
                    vec![
                        widgets::value_row("Clarion", env!("CARGO_PKG_VERSION"), p, cx)
                            .into_any_element(),
                        widgets::value_row("桌面连接", connected, p, cx).into_any_element(),
                    ],
                    p,
                ),
            ))
            .child(widgets::section(
                "桌面",
                p,
                cx,
                widgets::stack(
                    vec![
                        widgets::value_row("主题", theme, p, cx).into_any_element(),
                        widgets::value_row(
                            "程序坞",
                            format!("{:.0} pt · {position}", self.prefs.dock_size),
                            p,
                            cx,
                        )
                        .into_any_element(),
                        widgets::value_row(
                            "菜单栏",
                            format!("{:.0} pt", self.prefs.bar_height),
                            p,
                            cx,
                        )
                        .into_any_element(),
                    ],
                    p,
                ),
            ))
            .child(widgets::section(
                "文件",
                p,
                cx,
                widgets::stack(
                    vec![
                        widgets::row("偏好文件", preferences_control, p, cx)
                            .into_any_element(),
                        widgets::row("配置文件", config_control, p, cx).into_any_element(),
                    ],
                    p,
                ),
            ))
            .into_any_element()
    }

    fn appearance(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let light_entity = entity.clone();
        let dark_entity = entity.clone();
        let mut accents = div().flex().items_center().gap(px(8.)).py(px(8.)).px(px(10.));
        for color in [
            "#0a84ff", "#bf5af2", "#ff375f", "#ff453a", "#ff9f0a", "#ffd60a",
            "#30d158", "#8e8e93",
        ] {
            let swatch_entity = entity.clone();
            accents = accents.child(widgets::swatch(
                SharedString::from(format!("accent-{}", &color[1..])),
                color,
                self.prefs.accent.eq_ignore_ascii_case(color),
                p,
                move |_, cx| {
                    change(&swatch_entity, cx, |prefs| prefs.accent = color.into());
                },
            ));
        }
        let custom_accent = entity.clone();
        accents = accents.child(widgets::custom_color_swatch(
            "accent-custom",
            p,
            move |_, cx| {
                custom_accent.update(cx, |app, cx| app.open_accent_picker(cx));
            },
        ));

        let dock_opacity_entity = entity.clone();
        let surface_opacity_entity = entity.clone();
        let frost_entity = entity.clone();
        let theme_wallpaper = self
            .wallpaper_current
            .as_ref()
            .and_then(|path| self.wallpaper_thumbs.get(path))
            .map(|thumbs| thumbs.theme.clone());
        div()
            .flex()
            .flex_col()
            .child(widgets::section(
                "外观",
                p,
                cx,
                div()
                    .flex()
                    .w_full()
                    .justify_center()
                    .gap(px(64.))
                    .px(px(12.))
                    .py(px(12.))
                    .child(widgets::theme_card(
                        "theme-light",
                        "浅色",
                        false,
                        self.prefs.theme == "light",
                        theme_wallpaper.clone(),
                        p.accent,
                        p,
                        cx,
                        move |_, cx| {
                            change(&light_entity, cx, |prefs| prefs.theme = "light".into());
                            light_entity.update(cx, |app, cx| app.sync_theme_variants(cx));
                        },
                    ))
                    .child(widgets::theme_card(
                        "theme-dark",
                        "深色",
                        true,
                        self.prefs.theme != "light",
                        theme_wallpaper,
                        p.accent,
                        p,
                        cx,
                        move |_, cx| {
                            change(&dark_entity, cx, |prefs| prefs.theme = "dark".into());
                            dark_entity.update(cx, |app, cx| app.sync_theme_variants(cx));
                        },
                    )),
            ))
            .child({
                let dir_entity = entity.clone();
                let dir_text = if self.prefs.wallpaper_dir.is_empty() {
                    "默认图片目录".to_owned()
                } else {
                    short_path(&self.prefs.wallpaper_dir)
                };
                let dir_control = div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(widgets::label(dir_text, 11., FontWeight::NORMAL, p.muted, cx))
                    .child(widgets::button(
                        "wallpaper-dir",
                        "选择…",
                        p,
                        cx,
                        move |_, cx| {
                            dir_entity.update(cx, |app, cx| app.choose_wallpaper_dir(cx));
                        },
                    ));
                let mut theme_rows = vec![accents.into_any_element()];
                theme_rows.push(
                    widgets::row_with_help(
                        "系统主题",
                        Some(self.help_hint(
                            "help-system-theme",
                            "GTK 应用使用的主题。Clarion 自己的浅色/深色外观仍由上方两张预览卡控制。",
                            p,
                            cx,
                        )),
                        self.dropdown_control(
                            entity,
                            "system-theme",
                            DropdownId::SystemTheme,
                            p,
                            cx,
                        ),
                        p,
                        cx,
                    )
                    .into_any_element(),
                );
                theme_rows.push(
                    widgets::row_with_help(
                        "图标主题",
                        Some(self.help_hint(
                            "help-icon-theme",
                            "Clarion Shell 和 GTK 应用使用的图标主题；切换后会请求 Shell 立即重新载入。",
                            p,
                            cx,
                        )),
                        self.dropdown_control(
                            entity,
                            "icon-theme",
                            DropdownId::IconTheme,
                            p,
                            cx,
                        ),
                        p,
                        cx,
                    )
                    .into_any_element(),
                );
                theme_rows.push(
                    widgets::row_with_help(
                        "壁纸目录",
                        Some(self.help_hint(
                            "help-wallpaper-dir",
                            "壁纸库和 Spotlight 的图片模式只扫描这个目录。留空时使用 ~/Pictures/Wallpapers 等传统目录；设置后只搜索所选目录。",
                            p,
                            cx,
                        )),
                        dir_control,
                        p,
                        cx,
                    )
                    .into_any_element(),
                );
                theme_rows.push(self.wallpaper_history_block(p, cx));
                widgets::section("主题", p, cx, widgets::stack(theme_rows, p))
            })
            .child(widgets::section(
                "半透明与磨砂",
                p,
                cx,
                widgets::stack(
                    vec![
                        widgets::slider_row(
                            "程序坞透明度",
                            "dock-opacity",
                            (1. - self.prefs.dock_opacity) * 100.,
                            (0., 70.),
                            format!("{:.0}%", (1. - self.prefs.dock_opacity) * 100.),
                            p,
                            cx,
                            move |value, _, cx| {
                                change(&dock_opacity_entity, cx, |prefs| {
                                    prefs.dock_opacity = (1. - value / 100.).clamp(
                                        MIN_SURFACE_OPACITY,
                                        MAX_SURFACE_OPACITY,
                                    )
                                });
                            },
                        )
                        .into_any_element(),
                        widgets::slider_row(
                            "菜单弹窗透明度",
                            "surface-opacity",
                            (1. - self.prefs.surface_opacity) * 100.,
                            (0., 70.),
                            format!("{:.0}%", (1. - self.prefs.surface_opacity) * 100.),
                            p,
                            cx,
                            move |value, _, cx| {
                                change(&surface_opacity_entity, cx, |prefs| {
                                    prefs.surface_opacity = (1. - value / 100.).clamp(
                                        MIN_SURFACE_OPACITY,
                                        MAX_SURFACE_OPACITY,
                                    )
                                });
                            },
                        )
                        .into_any_element(),
                        widgets::slider_row(
                            "磨砂强度",
                            "frost",
                            self.prefs.frost_strength * 100.,
                            (0., 100.),
                            format!("{:.0}%", self.prefs.frost_strength * 100.),
                            p,
                            cx,
                            move |value, _, cx| {
                                change(&frost_entity, cx, |prefs| {
                                    prefs.frost_strength = (value / 100.).clamp(0., 1.)
                                });
                            },
                        )
                        .into_any_element(),
                    ],
                    p,
                ),
            ))
            .child({
                let mut cursor = Vec::new();
                cursor.push(
                    widgets::row_with_help(
                        "鼠标主题",
                        Some(self.help_hint(
                            "help-cursor-theme",
                            "光标主题同时写入 Clarion 配置和 GTK 设置，桌面与 GTK 应用使用同一套指针。",
                            p,
                            cx,
                        )),
                        self.dropdown_control(
                            entity,
                            "cursor-theme",
                            DropdownId::CursorTheme,
                            p,
                            cx,
                        ),
                        p,
                        cx,
                    )
                    .into_any_element(),
                );
                {
                    let change = entity.clone();
                    let size = self.commons_draft.cursor_size;
                    cursor.push(
                        widgets::slider_row(
                            "光标大小",
                            "cursor-size",
                            f64::from(size),
                            (16., 64.),
                            format!("{size} px"),
                            p,
                            cx,
                            move |value, _, cx| {
                                common_change(&change, cx, |commons| {
                                    commons.cursor_size = value.round().clamp(16., 64.) as u8
                                })
                            },
                        )
                        .into_any_element(),
                    );
                }
                cursor.push(common_toggle_row(
                    "输入时隐藏光标",
                    "cursor-hide-typing",
                    self.commons_draft.cursor_hide_when_typing,
                    p,
                    cx,
                    entity.clone(),
                    |commons, value| commons.cursor_hide_when_typing = value,
                ));
                cursor.push(
                    widgets::row(
                        "无操作后隐藏",
                        self.dropdown_control(
                            entity,
                            "cursor-hide",
                            DropdownId::CursorHide,
                            p,
                            cx,
                        ),
                        p,
                        cx,
                    )
                    .into_any_element(),
                );
                widgets::section("光标", p, cx, widgets::stack(cursor, p))
            })
            .into_any_element()
    }

    fn dock(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut size_rows = Vec::new();
        size_rows.push(
            widgets::row(
                "图标大小",
                self.dropdown_control(entity, "dock-size", DropdownId::DockSize, p, cx),
                p,
                cx,
            )
            .into_any_element(),
        );
        let magnify = entity.clone();
        size_rows.push(
            widgets::row(
                "放大效果",
                widgets::toggle(
                    "dock-magnification",
                    self.prefs.magnification,
                    p,
                    move |value, _, cx| {
                        change(&magnify, cx, |prefs| prefs.magnification = value)
                    },
                ),
                p,
                cx,
            )
            .into_any_element(),
        );
        if self.prefs.magnification {
            let scale = entity.clone();
            size_rows.push(
                widgets::slider_row(
                    "放大比例",
                    "dock-scale",
                    self.prefs.magnification_scale * 100.,
                    (100., 200.),
                    format!("{:.0}%", self.prefs.magnification_scale * 100.),
                    p,
                    cx,
                    move |value, _, cx| {
                        change(&scale, cx, |prefs| {
                            prefs.magnification_scale = (value / 100.).clamp(1., 2.)
                        })
                    },
                )
                .into_any_element(),
            );
        }

        let autohide = entity.clone();
        let recents = entity.clone();
        let indicators = entity.clone();
        let thumbnails = entity.clone();
        let behavior = widgets::stack(
            vec![
                toggle_row(
                    "自动隐藏和显示程序坞",
                    "dock-autohide",
                    self.prefs.dock_auto_hide,
                    p,
                    cx,
                    autohide,
                    |prefs, value| prefs.dock_auto_hide = value,
                ),
                toggle_row(
                    "显示最近使用的应用",
                    "dock-recents",
                    self.prefs.show_recent_applications,
                    p,
                    cx,
                    recents,
                    |prefs, value| prefs.show_recent_applications = value,
                ),
                toggle_row(
                    "显示运行指示点",
                    "dock-indicators",
                    self.prefs.show_running_indicators,
                    p,
                    cx,
                    indicators,
                    |prefs, value| prefs.show_running_indicators = value,
                ),
                toggle_row(
                    "悬停显示窗口缩略图",
                    "dock-thumbnails",
                    self.prefs.dock_hover_thumbnails,
                    p,
                    cx,
                    thumbnails,
                    |prefs, value| prefs.dock_hover_thumbnails = value,
                ),
            ],
            p,
        );

        let animation = entity.clone();
        let launcher = widgets::stack(
            vec![
                widgets::row(
                    "启动台呈现",
                    self.dropdown_control(entity, "dock-launcher", DropdownId::Launcher, p, cx),
                    p,
                    cx,
                )
                .into_any_element(),
                toggle_row(
                    "启动应用时弹跳图标",
                    "dock-animation",
                    self.prefs.animate_opening_applications,
                    p,
                    cx,
                    animation,
                    |prefs, value| prefs.animate_opening_applications = value,
                ),
            ],
            p,
        );

        let launcher_flag = entity.clone();
        let downloads = entity.clone();
        let minimize = entity.clone();
        let icons = entity.clone();
        let more = widgets::stack(
            vec![
                toggle_row(
                    "显示启动台入口",
                    "dock-show-launcher",
                    self.prefs.show_launcher,
                    p,
                    cx,
                    launcher_flag,
                    |prefs, value| prefs.show_launcher = value,
                ),
                toggle_row(
                    "显示下载目录",
                    "dock-downloads",
                    self.prefs.show_downloads,
                    p,
                    cx,
                    downloads,
                    |prefs, value| prefs.show_downloads = value,
                ),
                toggle_row(
                    "最小化到应用图标",
                    "dock-minimize",
                    self.prefs.minimize_into_app,
                    p,
                    cx,
                    minimize,
                    |prefs, value| prefs.minimize_into_app = value,
                ),
                toggle_row(
                    "使用应用原始图标",
                    "dock-icons",
                    self.prefs.use_original_icons,
                    p,
                    cx,
                    icons,
                    |prefs, value| prefs.use_original_icons = value,
                ),
                widgets::row(
                    "最小化效果",
                    self.dropdown_control(
                        entity,
                        "dock-effect",
                        DropdownId::MinimizeEffect,
                        p,
                        cx,
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
            ],
            p,
        );

        let size = widgets::stack(size_rows, p);
        let position = widgets::stack(
            vec![widgets::row(
                "位置",
                self.dropdown_control(entity, "dock-position", DropdownId::DockPosition, p, cx),
                p,
                cx,
            )
            .into_any_element()],
            p,
        );

        div()
            .flex()
            .flex_col()
            .child(widgets::section("大小与放大", p, cx, size))
            .child(widgets::section("屏幕上的位置", p, cx, position))
            .child(widgets::section("行为", p, cx, behavior))
            .child(widgets::section("启动台与启动动画", p, cx, launcher))
            .child(widgets::section("更多", p, cx, more))
            .into_any_element()
    }

    fn display_skeleton(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let loading = self.display_loading;
        let (message, error) = self
            .display_message
            .as_ref()
            .map(|(message, error)| (message.clone(), *error))
            .unwrap_or_else(|| ("正在读取显示器信息…".into(), false));
        let refresh = entity.clone();
        let placeholder = |title: &'static str| {
            widgets::row(
                title,
                widgets::label("—", 11., FontWeight::NORMAL, p.muted, cx),
                p,
                cx,
            )
            .into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .child(widgets::section(
                "排列",
                p,
                cx,
                div()
                    .h(px(250.))
                    .w_full()
                    .rounded(px(12.))
                    .bg(p.chip)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(widgets::label(
                        message.clone(),
                        11.,
                        FontWeight::NORMAL,
                        if error { rgb(0xff453a).into() } else { p.muted },
                        cx,
                    )),
            ))
            .child(
                div()
                    .mb(px(24.))
                    .child(widgets::card(
                        widgets::row(
                            if loading {
                                "正在读取显示器…"
                            } else {
                                "没有可用的显示器"
                            },
                            widgets::label("—", 11., FontWeight::NORMAL, p.muted, cx),
                            p,
                            cx,
                        ),
                        p,
                    )),
            )
            .child(widgets::section(
                "分辨率与缩放",
                p,
                cx,
                widgets::stack(
                    vec![
                        placeholder("分辨率"),
                        placeholder("刷新率"),
                        placeholder("界面缩放"),
                        placeholder("方向"),
                    ],
                    p,
                ),
            ))
            .child(widgets::section(
                "色彩与动态范围",
                p,
                cx,
                widgets::stack(
                    vec![
                        placeholder("高动态范围（HDR）"),
                        placeholder("可变刷新率"),
                    ],
                    p,
                ),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(widgets::button(
                        "display-refresh",
                        "刷新",
                        p,
                        cx,
                        move |_, cx| {
                            refresh.update(cx, |this, cx| this.refresh_displays(cx));
                        },
                    )),
            )
            .into_any_element()
    }

    fn displays_page(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(page) = self.display.as_ref() else {
            return self.display_skeleton(entity, p, cx);
        };

        let output = page
            .outputs
            .iter()
            .find(|output| Some(&output.name) == page.selected.as_ref());
        let active_outputs = page
            .outputs
            .iter()
            .filter(|output| output.logical.is_some())
            .count();
        let only_active = output.is_some_and(|output| output.logical.is_some())
            && active_outputs <= 1;
        let hdr_supported = output.is_some_and(|output| output.hdr_supported);
        let vrr_supported = output.is_some_and(|output| output.vrr_supported);

        let mut root = div().flex().flex_col();

        let active_outputs_with_layout = page
            .outputs
            .iter()
            .filter(|output| output.logical.is_some())
            .count();
        if active_outputs_with_layout > 1 {
            let canvas_size = self
                .arrangement_bounds
                .borrow()
                .map(|bounds| bounds.size)
                .unwrap_or_else(|| size(px(758.), px(250.)));
            let fixed = self
                .arrangement_drag
                .as_ref()
                .map(|drag| (drag.scale, drag.offset.0, drag.offset.1));
            let (rects, _, _, _) = arrangement_layout(
                &page.outputs,
                self.arrangement_positions.as_deref(),
                canvas_size,
                fixed,
            );
            let mut canvas = div()
                .id("display-arrangement")
                .debug_selector(|| "display-arrangement".into())
                .relative()
                .w_full()
                .h(px(250.))
                .overflow_hidden();
            let slot = self.arrangement_bounds.clone();
            canvas = canvas.child(
                gpui::canvas(
                    move |bounds, _, _| {
                        if f32::from(bounds.size.width) >= 100.
                            && f32::from(bounds.size.height) >= 50.
                        {
                            *slot.borrow_mut() = Some(bounds);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            );
            for (name, x, y, width, height) in rects {
                let selected = page.selected.as_deref() == Some(name.as_str());
                let resolution = page
                    .outputs
                    .iter()
                    .find(|output| output.name == name)
                    .and_then(|output| {
                        output
                            .current_mode
                            .and_then(|index| output.modes.get(index))
                            .map(|mode| format!("{} × {}", mode.width, mode.height))
                    })
                    .unwrap_or_default();
                let drag_entity = entity.clone();
                let drag_name = name.clone();
                let label_name = name.clone();
                canvas = canvas.child(
                    div()
                        .id(SharedString::from(format!("display-arrange-{name}")))
                        .debug_selector({
                            let name = name.clone();
                            move || format!("display-arrange-{name}")
                        })
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(width.max(56.)))
                        .h(px(height.max(36.)))
                        .rounded(px(10.))
                        .border_2()
                        .border_color(if selected { p.accent } else { p.control_border })
                        .bg(p.chip)
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(2.))
                        .cursor_pointer()
                        .hover(move |div| div.bg(p.chip_hover))
                        .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                            drag_entity.update(cx, |this, cx| {
                                this.begin_arrangement_drag(
                                    drag_name.clone(),
                                    event.position,
                                    cx,
                                )
                            });
                            cx.stop_propagation();
                        })
                        .child(widgets::label(
                            label_name,
                            10.,
                            FontWeight::MEDIUM,
                            p.text,
                            cx,
                        ))
                        .when(!resolution.is_empty(), |el| {
                            el.child(widgets::label(
                                resolution,
                                9.,
                                FontWeight::NORMAL,
                                p.muted,
                                cx,
                            ))
                        }),
                );
            }
            let move_entity = entity.clone();
            let up_entity = entity.clone();
            canvas = canvas
                .on_mouse_move(move |event: &gpui::MouseMoveEvent, _, cx| {
                    move_entity.update(cx, |this, cx| {
                        this.update_arrangement_drag(event.position, cx)
                    });
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    up_entity.update(cx, |this, cx| this.finish_arrangement_drag(cx));
                });
            root = root.child(widgets::section_hint(
                "排列",
                "点击选择 · 拖动调整位置",
                p,
                cx,
                canvas,
            ));
        }

        let full_name = output
            .map(|output| {
                format!(
                    "{} · {} {}",
                    output.name, output.make, output.model
                )
            })
            .unwrap_or_else(|| "选择显示器".into());
        let enabled_control = if only_active {
            widgets::label(
                "唯一的活动显示器",
                11.,
                FontWeight::NORMAL,
                p.muted,
                cx,
            )
            .into_any_element()
        } else {
            let change = entity.clone();
            widgets::toggle(
                "display-enabled",
                page.draft.enabled,
                p,
                move |value, _, cx| {
                    display_change(&change, cx, |draft| draft.enabled = value)
                },
            )
            .into_any_element()
        };
        root = root.child(
            div()
                .mb(px(24.))
                .child(widgets::card(
                    widgets::row(full_name, enabled_control, p, cx),
                    p,
                )),
        );

        let geometry = widgets::stack(
            vec![
                widgets::row(
                    "分辨率",
                    self.dropdown_control(
                        entity,
                        "display-resolution",
                        DropdownId::DisplayResolution,
                        p,
                        cx,
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
                widgets::row(
                    "刷新率",
                    self.dropdown_control(
                        entity,
                        "display-refresh-rate",
                        DropdownId::DisplayRefresh,
                        p,
                        cx,
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
                widgets::row(
                    "界面缩放",
                    self.dropdown_control(
                        entity,
                        "display-scale",
                        DropdownId::DisplayScale,
                        p,
                        cx,
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
                widgets::row(
                    "方向",
                    self.dropdown_control(
                        entity,
                        "display-transform",
                        DropdownId::DisplayTransform,
                        p,
                        cx,
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
            ],
            p,
        );
        root = root.child(widgets::section("分辨率与缩放", p, cx, geometry));

        let hdr_available = hdr_supported || page.draft.hdr;
        let hdr_sliders = hdr_supported && page.draft.hdr;
        let mut colour = Vec::new();
        {
            let change = entity.clone();
            colour.push(
                widgets::row(
                    "高动态范围（HDR）",
                    widgets::toggle_state(
                        "display-hdr",
                        page.draft.hdr,
                        hdr_available,
                        p,
                        move |value, _, cx| {
                            display_change(&change, cx, |draft| draft.hdr = value)
                        },
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
            );
        }
        if let Some(status) = output
            .and_then(|output| output.hdr_status.as_deref())
            .filter(|status| {
                !status.is_empty() && *status != "disabled" && *status != "enabled"
            })
        {
            colour.push(
                div().px(px(12.)).pb(px(4.)).child(widgets::label(
                    status.to_owned(),
                    11.,
                    FontWeight::NORMAL,
                    p.muted,
                    cx,
                ))
                .into_any_element(),
            );
        }
        {
            let change = entity.clone();
            let white = page.draft.sdr_white_level.unwrap_or(203.);
            colour.push(
                widgets::slider_row_state(
                    "SDR 内容白亮度",
                    "display-sdr-white",
                    white,
                    (80., 500.),
                    format!("{white:.0} 尼特"),
                    hdr_sliders,
                    p,
                    cx,
                    move |value, _, cx| {
                        display_change(&change, cx, |draft| {
                            draft.sdr_white_level = Some(value.clamp(80., 500.))
                        })
                    },
                )
                .into_any_element(),
            );
        }
        {
            let change = entity.clone();
            let brightness = page.draft.hdr_brightness.unwrap_or(100.);
            colour.push(
                widgets::slider_row_state(
                    "HDR 输出亮度",
                    "display-hdr-brightness",
                    brightness,
                    (1., 100.),
                    format!("{brightness:.0}%"),
                    hdr_sliders,
                    p,
                    cx,
                    move |value, _, cx| {
                        display_change(&change, cx, |draft| {
                            draft.hdr_brightness = Some(value.clamp(1., 100.))
                        })
                    },
                )
                .into_any_element(),
            );
        }
        colour.push(
            widgets::row(
                "色深",
                self.dropdown_control(
                    entity,
                    "display-max-bpc",
                    DropdownId::DisplayMaxBpc,
                    p,
                    cx,
                ),
                p,
                cx,
            )
            .into_any_element(),
        );
        {
            let vrr_available = vrr_supported || page.draft.vrr != VrrMode::Off;
            colour.push(
                widgets::row(
                    "可变刷新率",
                    self.dropdown_control_state(
                        entity,
                        "display-vrr",
                        DropdownId::DisplayVrr,
                        vrr_available,
                        p,
                        cx,
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
            );
        }
        root = root.child(widgets::section(
            "色彩与动态范围",
            p,
            cx,
            widgets::stack(colour, p),
        ));

        let focus_entity = entity.clone();
        let focus_toggle = widgets::toggle_state(
            "display-focus-startup",
            page.draft.focus_at_startup,
            self.display_preview.is_none(),
            p,
            move |value, _, cx| {
                display_config_only(&focus_entity, cx, |draft| {
                    draft.focus_at_startup = value
                })
            },
        );
        root = root.child(widgets::section(
            "启动",
            p,
            cx,
            widgets::row(
                "登录时聚焦此显示器",
                focus_toggle,
                p,
                cx,
            ),
        ));

        if self.display_preview.is_none() {
            if let Some((message, error)) = &self.display_message {
                root = root.child(widgets::label(
                    message.clone(),
                    11.,
                    FontWeight::NORMAL,
                    if *error { rgb(0xff453a).into() } else { p.muted },
                    cx,
                ));
            }
        }

        let refresh = entity.clone();
        root = root.child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(widgets::button("display-refresh", "刷新", p, cx, move |_, cx| {
                    refresh.update(cx, |this, cx| this.refresh_displays(cx));
                }))
                .child(self.help_hint(
                    "help-display-preview",
                    "修改会自动预览；15 秒内未确认会恢复。屏幕排列仍在旧版设置中调整。",
                    p,
                    cx,
                )),
        );

        root.into_any_element()
    }

    fn mission_control(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let spread = entity.clone();
        let overview = entity.clone();
        let toolbar = div()
            .flex()
            .gap(px(8.))
            .child(widgets::button(
                "mission-spread",
                "全窗口展开",
                p,
                cx,
                move |_, cx| {
                    spread.update(cx, |this, cx| {
                        this.desktop_action(clarion_ipc::Action::ToggleWindowSpread {}, cx)
                    });
                },
            ))
            .child(widgets::button(
                "mission-overview",
                "概览",
                p,
                cx,
                move |_, cx| {
                    overview.update(cx, |this, cx| {
                        this.desktop_action(clarion_ipc::Action::ToggleOverview {}, cx)
                    });
                },
            ));

        let stage_manager = widgets::stack(
            vec![{
                let change = entity.clone();
                widgets::row_with_help(
                    "台前调度",
                    Some(self.help_hint(
                        "help-stage-manager",
                        "当前窗口独占工作区，其余窗口收进左侧浮动栏。鼠标靠近屏幕左缘时浮动栏滑出，点击缩略图切换窗口；壁纸与窗口位置不受影响。",
                        p,
                        cx,
                    )),
                    widgets::toggle(
                        "mission-stage-manager",
                        self.commons_draft.stage_manager_enabled,
                        p,
                        move |value, _, cx| {
                            common_change(&change, cx, |commons| {
                                commons.stage_manager_enabled = value
                            })
                        },
                    ),
                    p,
                    cx,
                )
                .into_any_element()
            }],
            p,
        );
        let mut overview_rows = Vec::new();
        overview_rows.push(
            widgets::row(
                "概览风格",
                self.dropdown_control(
                    entity,
                    "mission-overview-style",
                    DropdownId::OverviewStyle,
                    p,
                    cx,
                ),
                p,
                cx,
            )
            .into_any_element(),
        );
        {
            let change = entity.clone();
            let zoom = self.commons_draft.overview_zoom;
            overview_rows.push(
                widgets::slider_row(
                    "概览缩放",
                    "mission-overview-zoom",
                    zoom * 100.,
                    (10., 100.),
                    format!("{:.0}%", zoom * 100.),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.overview_zoom = (value / 100.).clamp(0.1, 1.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        overview_rows.push(self.color_row(
            ColorKey::OverviewBackdrop,
            "概览背景色",
            p,
            cx,
        ));
        overview_rows.push(common_toggle_row(
            "工作区阴影",
            "overview-shadow",
            self.commons_draft.overview_shadow_enabled,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.overview_shadow_enabled = value,
        ));
        for (title, id, value, range, set) in [
            (
                "阴影水平偏移",
                "overview-shadow-x",
                self.commons_draft.overview_shadow_offset_x,
                (-200., 200.),
                (|commons: &mut CommonSettings, value| commons.overview_shadow_offset_x = value)
                    as fn(&mut CommonSettings, f64),
            ),
            (
                "阴影垂直偏移",
                "overview-shadow-y",
                self.commons_draft.overview_shadow_offset_y,
                (-200., 200.),
                |commons, value| commons.overview_shadow_offset_y = value,
            ),
            (
                "阴影柔和度",
                "overview-shadow-softness",
                self.commons_draft.overview_shadow_softness,
                (0., 200.),
                |commons, value| commons.overview_shadow_softness = value,
            ),
            (
                "阴影扩散",
                "overview-shadow-spread",
                self.commons_draft.overview_shadow_spread,
                (-64., 64.),
                |commons, value| commons.overview_shadow_spread = value,
            ),
        ] {
            let change = entity.clone();
            overview_rows.push(
                widgets::slider_row(
                    title,
                    id,
                    value,
                    range,
                    format!("{value:.0} px"),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| set(commons, value))
                    },
                )
                .into_any_element(),
            );
        }
        overview_rows.push(self.color_row(
            ColorKey::OverviewShadow,
            "工作区阴影颜色",
            p,
            cx,
        ));
        let overview = widgets::stack(overview_rows, p);

                let enhancement = widgets::stack(
            vec![
                common_toggle_row(
                    "弹性窗口",
                    "fx-wobble",
                    self.commons_draft.wobble_enabled,
                    p,
                    cx,
                    entity.clone(),
                    |commons, value| commons.wobble_enabled = value,
                ),
                common_toggle_row(
                    "立方体切换",
                    "fx-cube",
                    self.commons_draft.cube_enabled,
                    p,
                    cx,
                    entity.clone(),
                    |commons, value| commons.cube_enabled = value,
                ),
                common_toggle_row(
                    "窗口立体展开",
                    "fx-glide",
                    self.commons_draft.glide_enabled,
                    p,
                    cx,
                    entity.clone(),
                    |commons, value| commons.glide_enabled = value,
                ),
            ],
            p,
        );

        div()
            .flex()
            .flex_col()
            .child(widgets::section("桌面操作", p, cx, toolbar))
            .child(widgets::section("台前调度", p, cx, stage_manager))
            .child(widgets::section("概览", p, cx, overview))
            .child(widgets::section("增强效果", p, cx, enhancement))
            .into_any_element()
    }

    fn window_effects(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut motion = Vec::new();
        motion.push(common_toggle_row(
            "桌面动画",
            "fx-animations",
            self.commons_draft.animations_enabled,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.animations_enabled = value,
        ));
        {
            let change = entity.clone();
            motion.push(
                widgets::slider_row(
                    "动画时长",
                    "fx-speed",
                    self.commons_draft.animation_speed,
                    (0.25, 2.),
                    format!("{:.2} ×", self.commons_draft.animation_speed),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.animation_speed = value.clamp(0.25, 2.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        let motion = widgets::stack(motion, p);

        let window_motion = widgets::stack(
            vec![
                common_toggle_row(
                    "打开窗口",
                    "fx-window-open",
                    self.commons_draft.window_open_enabled,
                    p,
                    cx,
                    entity.clone(),
                    |commons, value| commons.window_open_enabled = value,
                ),
                widgets::row(
                    "打开效果",
                    self.dropdown_control(entity, "fx-open-effect", DropdownId::OpenEffect, p, cx),
                    p,
                    cx,
                )
                .into_any_element(),
                common_toggle_row(
                    "关闭窗口",
                    "fx-window-close",
                    self.commons_draft.window_close_enabled,
                    p,
                    cx,
                    entity.clone(),
                    |commons, value| commons.window_close_enabled = value,
                ),
                widgets::row(
                    "关闭效果",
                    self.dropdown_control(
                        entity,
                        "fx-close-effect",
                        DropdownId::CloseEffect,
                        p,
                        cx,
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
                common_toggle_row(
                    "窗口移动与平铺",
                    "fx-window-move",
                    self.commons_draft.window_movement_enabled,
                    p,
                    cx,
                    entity.clone(),
                    |commons, value| commons.window_movement_enabled = value,
                ),
                common_toggle_row(
                    "窗口尺寸过渡",
                    "fx-window-resize",
                    self.commons_draft.window_resize_enabled,
                    p,
                    cx,
                    entity.clone(),
                    |commons, value| commons.window_resize_enabled = value,
                ),
                common_toggle_row(
                    "切换工作区",
                    "fx-workspace",
                    self.commons_draft.workspace_switch_enabled,
                    p,
                    cx,
                    entity.clone(),
                    |commons, value| commons.workspace_switch_enabled = value,
                ),
            ],
            p,
        );

        let shape = widgets::stack(
            vec![{
                let change = entity.clone();
                widgets::slider_row(
                    "窗口圆角",
                    "fx-rounding",
                    self.commons_draft.rounding,
                    (0., 32.),
                    format!("{:.0} px", self.commons_draft.rounding),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.rounding = value.clamp(0., 32.)
                        })
                    },
                )
                .into_any_element()
            }],
            p,
        );

        let mut border_rows = Vec::new();
        {
            let change = entity.clone();
            border_rows.push(
                widgets::row_with_help(
                    "窗口边框",
                    Some(self.help_hint(
                        "help-border",
                        "边框围绕所有窗口绘制。关闭时下方的宽度、颜色与渐变会保留，重新开启后生效。",
                        p,
                        cx,
                    )),
                    widgets::toggle(
                        "fx-border",
                        self.commons_draft.border_enabled,
                        p,
                        move |value, _, cx| {
                            common_change(&change, cx, |commons| {
                                commons.border_enabled = value
                            })
                        },
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
            );
        }
        {
            let change = entity.clone();
            border_rows.push(
                widgets::slider_row(
                    "边框宽度",
                    "fx-border-width",
                    self.commons_draft.border_width,
                    (0.5, 12.),
                    format!("{:.1} px", self.commons_draft.border_width),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.border_width = value.clamp(0.5, 12.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        border_rows.push(self.color_row(
            ColorKey::BorderActive,
            "活动边框颜色",
            p,
            cx,
        ));
        border_rows.push(self.color_row(
            ColorKey::BorderInactive,
            "非活动边框颜色",
            p,
            cx,
        ));
        border_rows.push(self.color_row(
            ColorKey::BorderUrgent,
            "紧急边框颜色",
            p,
            cx,
        ));
        {
            let change = entity.clone();
            border_rows.push(
                widgets::row_with_help(
                    "活动窗口渐变",
                    Some(self.help_hint(
                        "help-border-gradient",
                        "渐变只替代活动窗口的边框颜色；非活动与紧急窗口继续使用各自的纯色。关闭后活动窗口恢复纯色边框。",
                        p,
                        cx,
                    )),
                    widgets::toggle(
                        "fx-border-gradient",
                        self.commons_draft.border_gradient_enabled,
                        p,
                        move |value, _, cx| {
                            common_change(&change, cx, |commons| {
                                commons.border_gradient_enabled = value
                            })
                        },
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
            );
        }
        if self.commons_draft.border_gradient_enabled {
            let from_color = parse_color_text(&self.commons_draft.border_gradient_from)
                .map(color_to_hsla)
                .unwrap_or_else(|| p.accent.opacity(0.8));
            let to_color = parse_color_text(&self.commons_draft.border_gradient_to)
                .map(color_to_hsla)
                .unwrap_or_else(|| p.accent.opacity(0.4));
            let angle = self.commons_draft.border_gradient_angle.clamp(0., 360.);
            border_rows.push(
                widgets::row(
                    "渐变颜色",
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(self.color_swatch_element(
                            ColorKey::BorderGradientFrom,
                            p,
                            cx,
                        ))
                        .child(
                            div()
                                .w(px(104.))
                                .h(px(10.))
                                .rounded(px(5.))
                                .border_1()
                                .border_color(p.control_border)
                                .bg(linear_gradient(
                                    angle as f32,
                                    linear_color_stop(from_color, 0.),
                                    linear_color_stop(to_color, 1.),
                                )),
                        )
                        .child(self.color_swatch_element(
                            ColorKey::BorderGradientTo,
                            p,
                            cx,
                        )),
                    p,
                    cx,
                )
                .into_any_element(),
            );
            let angle_change = entity.clone();
            let angle_dial = widgets::dial(
                "fx-gradient-angle",
                angle / 360.,
                format!("{angle:.0}°"),
                p,
                cx,
                move |fraction, _, cx| {
                    common_change(&angle_change, cx, |commons| {
                        commons.border_gradient_angle = (fraction * 360.).clamp(0., 360.)
                    })
                },
            );
            let speed = self.commons_draft.border_gradient_speed;
            let speed_change = entity.clone();
            let speed_dial = widgets::dial(
                "fx-gradient-speed",
                speed.abs() / 360.,
                if speed.abs() < 0.5 {
                    "静止".to_owned()
                } else {
                    format!("{:.0}°/s", speed.abs())
                },
                p,
                cx,
                move |fraction, _, cx| {
                    common_change(&speed_change, cx, |commons| {
                        commons.border_gradient_speed =
                            (fraction * 360.).abs().copysign(speed)
                    })
                },
            );
            border_rows.push(
                div()
                    .h(px(100.))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(20.))
                    .px(px(10.))
                    .child(widgets::label(
                        "渐变角度与旋转",
                        13.,
                        FontWeight::NORMAL,
                        p.text,
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(18.))
                            .child(self.dial_column("角度", angle_dial, p, cx))
                            .child(self.dial_column("旋转", speed_dial, p, cx)),
                    )
                    .into_any_element(),
            );
            border_rows.push(
                widgets::row(
                    "渐变范围",
                    self.dropdown_control(
                        entity,
                        "fx-gradient-relative",
                        DropdownId::GradientRelative,
                        p,
                        cx,
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
            );
        }
        let border = widgets::stack(border_rows, p);

        let mut ring_rows = Vec::new();
        {
            let change = entity.clone();
            ring_rows.push(
                widgets::row_with_help(
                    "焦点光环",
                    Some(self.help_hint(
                        "help-focus-ring",
                        "光环画在边框外侧，只围绕聚焦窗口，最大化和全屏时隐藏。活动光环颜色同时作为调度中心活动卡片的强调色；非活动光环颜色只在标签指示器继承配色时使用。关闭时光环设置会保留。",
                        p,
                        cx,
                    )),
                    widgets::toggle(
                        "fx-focus-ring",
                        self.commons_draft.focus_ring_enabled,
                        p,
                        move |value, _, cx| {
                            common_change(&change, cx, |commons| {
                                commons.focus_ring_enabled = value
                            })
                        },
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
            );
        }
        {
            let change = entity.clone();
            ring_rows.push(
                widgets::slider_row(
                    "光环宽度",
                    "fx-focus-width",
                    self.commons_draft.focus_ring_width,
                    (0.5, 12.),
                    format!("{:.1} px", self.commons_draft.focus_ring_width),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.focus_ring_width = value.clamp(0.5, 12.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        ring_rows.push(self.color_row(
            ColorKey::FocusActive,
            "活动光环颜色",
            p,
            cx,
        ));
        ring_rows.push(self.color_row(
            ColorKey::FocusInactive,
            "非活动光环颜色",
            p,
            cx,
        ));
        ring_rows.push(self.color_row(
            ColorKey::FocusUrgent,
            "紧急光环颜色",
            p,
            cx,
        ));
        let ring = widgets::stack(ring_rows, p);

        let mut shadow_rows = Vec::new();
        shadow_rows.push(common_toggle_row(
            "窗口阴影",
            "fx-shadow",
            self.commons_draft.shadow_enabled,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.shadow_enabled = value,
        ));
        shadow_rows.push(self.color_row(ColorKey::Shadow, "阴影颜色", p, cx));
        shadow_rows.push(self.color_row(
            ColorKey::ShadowInactive,
            "非活动阴影颜色",
            p,
            cx,
        ));
        for (title, id, value, range, set) in [
            (
                "阴影水平偏移",
                "fx-shadow-x",
                self.commons_draft.shadow_offset_x,
                (-64., 64.),
                (|commons: &mut CommonSettings, value| commons.shadow_offset_x = value)
                    as fn(&mut CommonSettings, f64),
            ),
            (
                "阴影垂直偏移",
                "fx-shadow-y",
                self.commons_draft.shadow_offset_y,
                (-64., 64.),
                |commons, value| commons.shadow_offset_y = value,
            ),
            (
                "阴影柔和度",
                "fx-shadow-softness",
                self.commons_draft.shadow_softness,
                (0., 128.),
                |commons, value| commons.shadow_softness = value,
            ),
            (
                "阴影扩散",
                "fx-shadow-spread",
                self.commons_draft.shadow_spread,
                (-32., 32.),
                |commons, value| commons.shadow_spread = value,
            ),
        ] {
            let change = entity.clone();
            shadow_rows.push(
                widgets::slider_row(
                    title,
                    id,
                    value,
                    range,
                    format!("{value:.0} px"),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| set(commons, value))
                    },
                )
                .into_any_element(),
            );
        }
        shadow_rows.push(common_toggle_row(
            "阴影绘制在窗口后方",
            "fx-shadow-behind",
            self.commons_draft.shadow_draw_behind,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.shadow_draw_behind = value,
        ));
        let shadow = widgets::stack(shadow_rows, p);

        let mut tabs = Vec::new();
        tabs.push(common_toggle_row(
            "显示标签指示器",
            "fx-tab-indicator",
            self.commons_draft.tab_indicator_enabled,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.tab_indicator_enabled = value,
        ));
        tabs.push(common_toggle_row(
            "只有一个标签时隐藏",
            "fx-tab-single",
            self.commons_draft.tab_indicator_hide_single,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.tab_indicator_hide_single = value,
        ));
        for (title, id, value, range, set) in [
            (
                "标签间距",
                "fx-tab-gap",
                self.commons_draft.tab_indicator_gap,
                (0., 64.),
                (|commons: &mut CommonSettings, value| commons.tab_indicator_gap = value)
                    as fn(&mut CommonSettings, f64),
            ),
            (
                "标签宽度",
                "fx-tab-width",
                self.commons_draft.tab_indicator_width,
                (0., 64.),
                |commons, value| commons.tab_indicator_width = value,
            ),
            (
                "标签圆角",
                "fx-tab-radius",
                self.commons_draft.tab_indicator_corner_radius,
                (0., 32.),
                |commons, value| commons.tab_indicator_corner_radius = value,
            ),
        ] {
            let change = entity.clone();
            tabs.push(
                widgets::slider_row(
                    title,
                    id,
                    value,
                    range,
                    format!("{value:.0} px"),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| set(commons, value))
                    },
                )
                .into_any_element(),
            );
        }
        tabs.push(self.color_row(
            ColorKey::TabActive,
            "活动标签颜色",
            p,
            cx,
        ));
        tabs.push(self.color_row(
            ColorKey::TabInactive,
            "非活动标签颜色",
            p,
            cx,
        ));
        tabs.push(self.color_row(
            ColorKey::TabUrgent,
            "紧急标签颜色",
            p,
            cx,
        ));
        let tabs = widgets::stack(tabs, p);

        let insert = widgets::stack(
            vec![
                common_toggle_row(
                    "显示插入提示",
                    "fx-insert-hint",
                    self.commons_draft.insert_hint_enabled,
                    p,
                    cx,
                    entity.clone(),
                    |commons, value| commons.insert_hint_enabled = value,
                ),
                self.color_row(ColorKey::InsertHint, "插入提示颜色", p, cx),
            ],
            p,
        );

        let mut blur = Vec::new();
        blur.push(common_toggle_row(
            "背景模糊",
            "fx-blur",
            self.commons_draft.blur_enabled,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.blur_enabled = value,
        ));
        {
            let change = entity.clone();
            blur.push(
                widgets::slider_row(
                    "模糊层次",
                    "fx-blur-passes",
                    f64::from(self.commons_draft.blur_passes),
                    (1., 8.),
                    format!("{} 层", self.commons_draft.blur_passes),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.blur_passes = value.round().clamp(1., 8.) as u8
                        })
                    },
                )
                .into_any_element(),
            );
        }
        {
            let change = entity.clone();
            blur.push(
                widgets::slider_row(
                    "模糊范围",
                    "fx-blur-offset",
                    self.commons_draft.blur_offset,
                    (0.5, 12.),
                    format!("{:.1} px", self.commons_draft.blur_offset),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.blur_offset = value.clamp(0.5, 12.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        {
            let change = entity.clone();
            blur.push(
                widgets::slider_row(
                    "噪点",
                    "fx-blur-noise",
                    self.commons_draft.blur_noise,
                    (0., 1.),
                    format!("{:.3}", self.commons_draft.blur_noise),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.blur_noise = value.clamp(0., 1.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        {
            let change = entity.clone();
            blur.push(
                widgets::slider_row(
                    "饱和度",
                    "fx-blur-saturation",
                    self.commons_draft.blur_saturation,
                    (0., 3.),
                    format!("{:.2} ×", self.commons_draft.blur_saturation),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.blur_saturation = value.clamp(0., 3.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        let blur = widgets::stack(blur, p);

        let mut page = div()
            .flex()
            .flex_col()
            .child(widgets::section("窗口边框", p, cx, border))
            .child(widgets::section("焦点光环", p, cx, ring))
            .child(widgets::section("窗口阴影", p, cx, shadow))
            .child(widgets::section("窗口形状", p, cx, shape))
            .child(widgets::section("动效", p, cx, motion))
            .child(widgets::section("窗口与工作区", p, cx, window_motion))
            .child(widgets::section("标签指示器", p, cx, tabs))
            .child(widgets::section("插入提示", p, cx, insert))
            .child(widgets::section("背景模糊", p, cx, blur));
        if let Some(error) = &self.commons_error {
            page = page.child(widgets::label(
                error.clone(),
                11.,
                FontWeight::NORMAL,
                rgb(0xff453a).into(),
                cx,
            ));
        }
        page.into_any_element()
    }

    fn layout_page(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut tiling = Vec::new();
        tiling.push(common_toggle_row(
            "自动堆叠窗口",
            "layout-stack",
            self.commons_draft.auto_stack,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.auto_stack = value,
        ));
        {
            let change = entity.clone();
            tiling.push(
                widgets::slider_row(
                    "每列窗口上限",
                    "layout-max-windows",
                    f64::from(self.commons_draft.max_windows_per_column),
                    (1., 8.),
                    format!("{} 个", self.commons_draft.max_windows_per_column),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.max_windows_per_column = value.round().clamp(1., 8.) as u32
                        })
                    },
                )
                .into_any_element(),
            );
        }
        tiling.push(
            widgets::row(
                "新窗口默认宽度",
                self.dropdown_control(
                    entity,
                    "layout-default-width",
                    DropdownId::DefaultWidth,
                    p,
                    cx,
                ),
                p,
                cx,
            )
            .into_any_element(),
        );
        {
            let change = entity.clone();
            tiling.push(
                widgets::slider_row(
                    "窗口间距",
                    "layout-gaps",
                    self.commons_draft.gaps,
                    (0., 40.),
                    format!("{:.0} px", self.commons_draft.gaps),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.gaps = value.clamp(0., 40.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        let tiling = widgets::stack(tiling, p);

        let focus = widgets::stack(
            vec![
                widgets::row(
                    "聚焦列居中",
                    self.dropdown_control(
                        entity,
                        "layout-center",
                        DropdownId::CenterColumn,
                        p,
                        cx,
                    ),
                    p,
                    cx,
                )
                .into_any_element(),
                common_toggle_row(
                    "单列始终居中",
                    "layout-center-single",
                    self.commons_draft.center_single,
                    p,
                    cx,
                    entity.clone(),
                    |commons, value| commons.center_single = value,
                ),
            ],
            p,
        );

        let mut corner_rows = Vec::new();
        corner_rows.push(common_toggle_row(
            "启用触发角",
            "hot-corners-enabled",
            self.commons_draft.hot_corners_enabled,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.hot_corners_enabled = value,
        ));
        for (id, action_id, title, value, set) in [
            (
                "hot-corner-top-left",
                "hot-corner-top-left-action",
                "左上角",
                self.commons_draft.hot_corner_top_left,
                (|commons: &mut CommonSettings, value| commons.hot_corner_top_left = value)
                    as fn(&mut CommonSettings, bool),
            ),
            (
                "hot-corner-top-right",
                "hot-corner-top-right-action",
                "右上角",
                self.commons_draft.hot_corner_top_right,
                |commons, value| commons.hot_corner_top_right = value,
            ),
            (
                "hot-corner-bottom-left",
                "hot-corner-bottom-left-action",
                "左下角",
                self.commons_draft.hot_corner_bottom_left,
                |commons, value| commons.hot_corner_bottom_left = value,
            ),
            (
                "hot-corner-bottom-right",
                "hot-corner-bottom-right-action",
                "右下角",
                self.commons_draft.hot_corner_bottom_right,
                |commons, value| commons.hot_corner_bottom_right = value,
            ),
        ] {
            let change = entity.clone();
            let dropdown = match action_id {
                "hot-corner-top-left-action" => {
                    self.dropdown_control(entity, action_id, DropdownId::HotCornerTopLeft, p, cx)
                }
                "hot-corner-top-right-action" => self.dropdown_control(
                    entity,
                    action_id,
                    DropdownId::HotCornerTopRight,
                    p,
                    cx,
                ),
                "hot-corner-bottom-left-action" => self.dropdown_control(
                    entity,
                    action_id,
                    DropdownId::HotCornerBottomLeft,
                    p,
                    cx,
                ),
                _ => self.dropdown_control(
                    entity,
                    action_id,
                    DropdownId::HotCornerBottomRight,
                    p,
                    cx,
                ),
            };
            corner_rows.push(
                widgets::row(
                    title,
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(dropdown)
                        .child(widgets::toggle(id, value, p, move |value, _, cx| {
                            common_change(&change, cx, |commons| set(commons, value))
                        })),
                    p,
                    cx,
                )
                .into_any_element(),
            );
        }
        let corners = widgets::stack(corner_rows, p);

        let mut surface = Vec::new();
        for (title, id, value, set) in [
            (
                "左侧预留边距",
                "layout-strut-left",
                self.commons_draft.struts_left,
                (|commons: &mut CommonSettings, value| commons.struts_left = value)
                    as fn(&mut CommonSettings, f64),
            ),
            (
                "右侧预留边距",
                "layout-strut-right",
                self.commons_draft.struts_right,
                |commons, value| commons.struts_right = value,
            ),
            (
                "顶部预留边距",
                "layout-strut-top",
                self.commons_draft.struts_top,
                |commons, value| commons.struts_top = value,
            ),
            (
                "底部预留边距",
                "layout-strut-bottom",
                self.commons_draft.struts_bottom,
                |commons, value| commons.struts_bottom = value,
            ),
        ] {
            let change = entity.clone();
            surface.push(
                widgets::slider_row(
                    title,
                    id,
                    value,
                    (-200., 200.),
                    format!("{value:.0} px"),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| set(commons, value))
                    },
                )
                .into_any_element(),
            );
        }
        surface.push(common_toggle_row(
            "淡化非活动窗口",
            "layout-dim-inactive",
            self.commons_draft.dim_inactive,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.dim_inactive = value,
        ));
        {
            let change = entity.clone();
            let value = self.commons_draft.dim_factor;
            surface.push(
                widgets::slider_row(
                    "非活动窗口亮度",
                    "layout-dim-factor",
                    value * 100.,
                    (0., 100.),
                    format!("{:.0}%", value * 100.),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.dim_factor = (value / 100.).clamp(0., 1.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        surface.push(self.color_row(
            ColorKey::Background,
            "工作区背景色",
            p,
            cx,
        ));
        surface.push(self.color_row(
            ColorKey::FullscreenBackdrop,
            "全屏窗口背景色",
            p,
            cx,
        ));
        {
            let change = entity.clone();
            surface.push(
                widgets::slider_row(
                    "圆角柔和度",
                    "layout-rounding-power",
                    self.commons_draft.rounding_power,
                    (2., 10.),
                    format!("{:.1}", self.commons_draft.rounding_power),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.rounding_power = value.clamp(2., 10.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        let surface = widgets::stack(surface, p);
        let preset_widths = self.preset_editor(PresetList::ColumnWidths, p, cx);
        let preset_heights = self.preset_editor(PresetList::WindowHeights, p, cx);

        div()
            .flex()
            .flex_col()
            .child(widgets::section("自动排列", p, cx, tiling))
            .child(widgets::section("聚焦与居中", p, cx, focus))
            .child(widgets::section("预设列宽", p, cx, preset_widths))
            .child(widgets::section("预设窗高", p, cx, preset_heights))
            .child(widgets::section("屏幕边距与背景", p, cx, surface))
            .child(widgets::section("触发角", p, cx, corners))
            .into_any_element()
    }

    fn color_swatch_element(
        &self,
        key: ColorKey,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = key.value(&self.commons_draft);
        let empty = text.trim().is_empty();
        let color = parse_color_text(&text)
            .map(color_to_hsla)
            .unwrap_or_else(|| gpui::rgba(0x00000000).into());
        let entity = cx.entity();
        widgets::color_swatch(key.id(), color, empty, p, move |_, cx| {
            entity.update(cx, |this, cx| this.open_color_picker(key, cx));
        })
    }

    fn color_row(
        &self,
        key: ColorKey,
        title: &'static str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let swatch = self.color_swatch_element(key, p, cx);
        widgets::row(title, swatch, p, cx).into_any_element()
    }

    fn dial_column(
        &self,
        title: &'static str,
        dial: AnyElement,
        p: Palette,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .child(dial)
            .child(widgets::label(title, 10.5, FontWeight::NORMAL, p.muted, cx))
            .into_any_element()
    }

    fn set_system_theme(&mut self, family_name: &str) {
        let Some(family) = self
            .system_themes
            .iter()
            .find(|family| family.name == family_name)
            .cloned()
        else {
            return;
        };
        let variant = family.variant(self.prefs.theme != "light");
        match write_gtk_setting(&self.environment, "gtk-theme-name", &variant) {
            Ok(()) => {
                self.system_theme = variant.clone();
                self.system_theme_family = family.name;
                self.status = Some((
                    format!("系统主题已切换为 {variant}"),
                    false,
                    Instant::now(),
                ));
            }
            Err(error) => {
                self.status = Some((
                    format!("无法写入 GTK 主题设置：{error}"),
                    true,
                    Instant::now(),
                ));
            }
        }
    }

    fn set_icon_theme(&mut self, family_name: &str) {
        let Some(family) = self
            .icon_themes
            .iter()
            .find(|family| family.name == family_name)
            .cloned()
        else {
            return;
        };
        let variant = family.variant(self.prefs.theme != "light");
        match write_gtk_setting(&self.environment, "gtk-icon-theme-name", &variant) {
            Ok(()) => {
                self.icon_theme = variant.clone();
                self.icon_theme_family = family.name;
                self.status = match self.bridge.command("reload-icons") {
                    Ok(()) => Some(("图标主题已切换".into(), false, Instant::now())),
                    Err(_) => Some((
                        format!("图标主题已保存；重启 Clarion Shell 后生效（{variant}）"),
                        false,
                        Instant::now(),
                    )),
                };
            }
            Err(error) => {
                self.status = Some((
                    format!("无法写入 GTK 图标主题：{error}"),
                    true,
                    Instant::now(),
                ));
            }
        }
    }

    fn set_cursor_theme(&mut self, family_name: &str) {
        let Some(family) = self
            .cursor_themes
            .iter()
            .find(|family| family.name == family_name)
            .cloned()
        else {
            return;
        };
        let variant = family.variant(self.prefs.theme != "light");
        self.cursor_theme_family = family.name;
        self.commons_draft.cursor_theme = variant.clone();
        let _ = write_gtk_setting(&self.environment, "gtk-cursor-theme-name", &variant);
    }

    fn sync_theme_variants(&mut self, cx: &mut Context<Self>) {
        let dark = self.prefs.theme != "light";
        if let Some(family) = self
            .system_themes
            .iter()
            .find(|family| family.name == self.system_theme_family)
            .cloned()
        {
            let variant = family.variant(dark);
            if variant != self.system_theme {
                let _ = write_gtk_setting(&self.environment, "gtk-theme-name", &variant);
                self.system_theme = variant;
            }
        }
        if let Some(family) = self
            .icon_themes
            .iter()
            .find(|family| family.name == self.icon_theme_family)
            .cloned()
        {
            let variant = family.variant(dark);
            if variant != self.icon_theme {
                let _ = write_gtk_setting(&self.environment, "gtk-icon-theme-name", &variant);
                self.icon_theme = variant;
            }
            let _ = self.bridge.command("reload-icons");
        }
        if let Some(family) = self
            .cursor_themes
            .iter()
            .find(|family| family.name == self.cursor_theme_family)
            .cloned()
        {
            let variant = family.variant(dark);
            if variant != self.commons_draft.cursor_theme {
                self.commons_draft.cursor_theme = variant.clone();
                let _ = write_gtk_setting(&self.environment, "gtk-cursor-theme-name", &variant);
                self.queue_common(cx);
            }
        }
        cx.notify();
    }

    fn refresh_wallpaper_state(&mut self, cx: &mut Context<Self>) {
        let (current, history) = wallpaper_state(&self.environment);
        self.wallpaper_current = current;
        self.wallpaper_history = history;
        let missing = self
            .wallpaper_history
            .iter()
            .filter(|path| !self.wallpaper_thumbs.contains_key(*path))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() && !self.wallpaper_loading {
            self.wallpaper_loading = true;
            cx.spawn(async move |this, cx| {
                let thumbs = cx
                    .background_executor()
                    .spawn(async move {
                        missing
                            .into_iter()
                            .filter_map(|path| {
                                wallpaper_thumbnails(&path).map(|thumbs| (path, thumbs))
                            })
                            .collect::<HashMap<_, _>>()
                    })
                    .await;
                let _ = this.update(cx, |app, cx| {
                    app.wallpaper_thumbs.extend(thumbs);
                    app.wallpaper_loading = false;
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn choose_wallpaper_dir(&mut self, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择壁纸目录".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = prompt.await else {
                return;
            };
            let Some(directory) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |app, cx| {
                app.prefs.wallpaper_dir = directory.to_string_lossy().into_owned();
                app.apply(cx);
                app.wallpaper_thumbs.clear();
                app.refresh_wallpaper_state(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn choose_wallpaper_file(&mut self, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择壁纸".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = prompt.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let supported = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .is_some_and(|extension| matches!(extension.as_str(), "png" | "jpg" | "jpeg"));
            if !supported {
                let _ = this.update(cx, |app, cx| {
                    app.status = Some((
                        "请选择 PNG 或 JPEG 图片".into(),
                        true,
                        Instant::now(),
                    ));
                    cx.notify();
                });
                return;
            }
            let _ = this.update(cx, |app, cx| app.apply_wallpaper(path, cx));
        })
        .detach();
    }

    fn apply_wallpaper(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.wallpaper_current.as_ref() == Some(&path) {
            return;
        }
        match self.bridge.set_wallpaper(&path) {
            Ok(()) => {
                let thumbnail_path = path.clone();
                self.wallpaper_current = Some(path.clone());
                self.wallpaper_history.retain(|entry| entry != &path);
                self.wallpaper_history.insert(0, path);
                self.wallpaper_history.truncate(6);
                self.status = Some(("壁纸已应用".into(), false, Instant::now()));
                if !self.wallpaper_thumbs.contains_key(&thumbnail_path) {
                    cx.spawn(async move |this, cx| {
                        let decode_path = thumbnail_path.clone();
                        let thumbs = cx
                            .background_executor()
                            .spawn(async move { wallpaper_thumbnails(&decode_path) })
                            .await;
                        if let Some(thumbs) = thumbs {
                            let _ = this.update(cx, |app, cx| {
                                app.wallpaper_thumbs.insert(thumbnail_path, thumbs);
                                cx.notify();
                            });
                        }
                    })
                    .detach();
                }
            }
            Err(error) => {
                self.status = Some((format!("无法应用壁纸：{error:#}"), true, Instant::now()));
            }
        }
        cx.notify();
    }

    fn wallpaper_history_block(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut gallery = div().flex().items_center().gap(px(10.));
        if self.wallpaper_history.is_empty() {
            for index in 0..3 {
                gallery = gallery.child(
                    div()
                        .id(SharedString::from(format!("wallpaper-empty-{index}")))
                        .w(px(112.))
                        .h(px(63.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(p.control_border)
                        .bg(p.text.opacity(0.03)),
                );
            }
        } else {
            for (index, path) in self.wallpaper_history.iter().take(5).enumerate() {
                let selected = self.wallpaper_current.as_ref() == Some(path);
                let id = format!("wallpaper-history-{index}");
                let entity = cx.entity();
                let path = path.clone();
                let apply_path = path.clone();
                let mut tile = div()
                    .id(SharedString::from(id.clone()))
                    .debug_selector(move || id.clone())
                    .relative()
                    .w(px(112.))
                    .h(px(63.))
                    .rounded(px(8.))
                    .overflow_hidden()
                    .bg(p.control)
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |app, cx| {
                            app.apply_wallpaper(apply_path.clone(), cx)
                        });
                    });
                if let Some(thumbs) = self.wallpaper_thumbs.get(&path) {
                    tile = tile.child(
                        div()
                            .absolute()
                            .left(px(2.))
                            .right(px(2.))
                            .top(px(2.))
                            .bottom(px(4.))
                            .child(
                                img(ImageSource::Image(thumbs.history.clone()))
                                    .object_fit(ObjectFit::Fill)
                                    .size_full(),
                            ),
                    );
                }
                tile = tile.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(8.))
                        .border_2()
                        .border_color(if selected { p.accent } else { p.card_border }),
                );
                gallery = gallery.child(tile);
            }
        }
        let change_entity = cx.entity();
        gallery = gallery.child(
            div()
                .id("wallpaper-change")
                .debug_selector(|| "wallpaper-change".into())
                .w(px(112.))
                .h(px(63.))
                .rounded(px(8.))
                .border_1()
                .border_color(p.control_border)
                .bg(p.text.opacity(0.04))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(1.))
                .cursor_pointer()
                .hover(move |tile| tile.bg(p.hover))
                .child(widgets::label(
                    "+",
                    19.,
                    FontWeight::MEDIUM,
                    p.muted,
                    cx,
                ))
                .child(widgets::label(
                    "更换壁纸",
                    10.5,
                    FontWeight::NORMAL,
                    p.muted,
                    cx,
                ))
                .on_click(move |_, _, cx| {
                    change_entity.update(cx, |app, cx| app.choose_wallpaper_file(cx));
                }),
        );
        div()
            .px(px(10.))
            .pt(px(10.))
            .pb(px(16.))
            .flex()
            .flex_col()
            .gap(px(9.))
            .child(widgets::label(
                "最近使用",
                13.,
                FontWeight::NORMAL,
                p.text,
                cx,
            ))
            .child(gallery)
            .into_any_element()
    }

    fn help_hint(
        &self,
        id: &'static str,
        text: &'static str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity();
        widgets::help_hint(id, p, move |hovered, window, cx| {
            entity.update(cx, |app, cx| {
                if hovered {
                    app.help_hover = Some((id, text, window.mouse_position()));
                } else if app
                    .help_hover
                    .is_some_and(|(current, _, _)| current == id)
                {
                    app.help_hover = None;
                }
                cx.notify();
            });
        })
    }

    fn decoration_preview(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let border = self.commons_draft.border_enabled;
        let ring = self.commons_draft.focus_ring_enabled;
        let focused = match (border, ring) {
            (true, true) => "聚焦窗口 · 边框 + 光环",
            (true, false) => "聚焦窗口 · 边框",
            (false, true) => "聚焦窗口 · 光环",
            (false, false) => "聚焦窗口 · 无可见描边",
        };
        let unfocused = if border {
            "未聚焦窗口 · 边框"
        } else {
            "未聚焦窗口 · 无可见描边"
        };
        div()
            .w_full()
            .flex()
            .items_end()
            .justify_center()
            .gap(px(48.))
            .px(px(18.))
            .py(px(8.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .child(self.decoration_sample(true, p))
                    .child(widgets::label(
                        focused,
                        11.,
                        FontWeight::NORMAL,
                        p.muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .child(self.decoration_sample(false, p))
                    .child(widgets::label(
                        unfocused,
                        11.,
                        FontWeight::NORMAL,
                        p.muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn window_preview_panel(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let help = self.help_hint(
            "help-window-preview",
            "聚焦窗口同时绘制边框和外侧光环，未聚焦窗口只显示边框；渐变会替代对应状态的颜色。",
            p,
            cx,
        );
        div()
            .size_full()
            .relative()
            .flex()
            .items_center()
            .child(self.decoration_preview(p, cx))
            .child(div().absolute().top(px(10.)).right(px(18.)).child(help))
            .into_any_element()
    }

    fn decoration_sample(&self, active: bool, p: Palette) -> AnyElement {
        let radius = self.commons_draft.rounding.clamp(0., 32.) as f32;
        let border_enabled = self.commons_draft.border_enabled;
        let ring_enabled = active && self.commons_draft.focus_ring_enabled;
        let border_width = if border_enabled {
            self.commons_draft.border_width.clamp(0.5, 12.).min(8.) as f32
        } else {
            0.
        };
        let ring_width = if ring_enabled {
            self.commons_draft.focus_ring_width.clamp(0.5, 12.).min(8.) as f32
        } else {
            0.
        };
        let border_color = parse_color_text(if active {
            &self.commons_draft.border_active_color
        } else {
            &self.commons_draft.border_inactive_color
        })
        .map(color_to_hsla)
        .unwrap_or_else(|| p.accent.opacity(0.8));
        let ring_color = parse_color_text(&self.commons_draft.focus_ring_active_color)
            .map(color_to_hsla)
            .unwrap_or(p.accent);
        let mut sample = div()
            .flex()
            .flex_col()
            .w(px(168.))
            .h(px(96.))
            .rounded(px(radius))
            .bg(p.search_bg)
            .child(
                div()
                    .h(px(22.))
                    .w_full()
                    .rounded_t(px(radius))
                    .bg(p.control)
                    .border_b_1()
                    .border_color(p.separator),
            );
        if border_enabled {
            let background: Background =
                if active && self.commons_draft.border_gradient_enabled {
                    let from = parse_color_text(&self.commons_draft.border_gradient_from)
                        .map(color_to_hsla)
                        .unwrap_or(border_color);
                    let to = parse_color_text(&self.commons_draft.border_gradient_to)
                        .map(color_to_hsla)
                        .unwrap_or(border_color);
                    linear_gradient(
                        self.commons_draft.border_gradient_angle.clamp(0., 360.) as f32,
                        linear_color_stop(from, 0.),
                        linear_color_stop(to, 1.),
                    )
                } else {
                    border_color.into()
                };
            sample = div()
                .rounded(px(radius + border_width))
                .bg(background)
                .p(px(border_width))
                .child(sample);
        }
        if ring_enabled {
            sample = div()
                .rounded(px(radius + border_width + ring_width))
                .bg(ring_color)
                .p(px(ring_width))
                .child(sample);
        }
        let selector = if active {
            "decoration-sample-focused"
        } else {
            "decoration-sample-unfocused"
        };
        div()
            .id(SharedString::from(selector))
            .debug_selector(move || selector.into())
            .child(sample)
            .into_any_element()
    }

    fn preset_values(&self, list: PresetList) -> Vec<PresetSize> {
        let text = match list {
            PresetList::ColumnWidths => &self.commons_draft.preset_column_widths,
            PresetList::WindowHeights => &self.commons_draft.preset_window_heights,
        };
        parse_preset_sizes(text).unwrap_or_default()
    }

    fn set_preset_values(
        &mut self,
        list: PresetList,
        values: Vec<PresetSize>,
        cx: &mut Context<Self>,
    ) {
        let text = preset_sizes_text(&values);
        match list {
            PresetList::ColumnWidths => self.commons_draft.preset_column_widths = text,
            PresetList::WindowHeights => self.commons_draft.preset_window_heights = text,
        }
        self.queue_common(cx);
        cx.notify();
    }

    fn add_preset(&mut self, list: PresetList, cx: &mut Context<Self>) {
        let mut values = self.preset_values(list);
        if values.len() >= 16 {
            return;
        }
        values.push(PresetSize::Proportion(0.5));
        let index = values.len() - 1;
        self.set_preset_values(list, values, cx);
        self.preset_selection = Some(PresetSelection { list, index });
        cx.notify();
    }

    fn remove_selected_preset(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = self.preset_selection else {
            return;
        };
        let mut values = self.preset_values(selection.list);
        if selection.index >= values.len() {
            return;
        }
        values.remove(selection.index);
        let next = if values.is_empty() {
            None
        } else {
            Some(PresetSelection {
                list: selection.list,
                index: selection.index.min(values.len() - 1),
            })
        };
        self.preset_selection = next;
        self.set_preset_values(selection.list, values, cx);
    }

    fn preset_editor(
        &self,
        list: PresetList,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let values = self.preset_values(list);
        let entity = cx.entity();
        let selected = self.preset_selection.filter(|selection| selection.list == list);
        let reference = match list {
            PresetList::ColumnWidths => 1920.,
            PresetList::WindowHeights => 1080.,
        };
        let ratio = |size: &PresetSize| match size {
            PresetSize::Proportion(value) => *value as f32,
            PresetSize::Fixed(value) => (f64::from(*value) / reference) as f32,
        };
        let max_ratio = values
            .iter()
            .map(ratio)
            .fold(0.01_f32, f32::max)
            .max(0.01);
        let mut tiles = div()
            .id(if list == PresetList::ColumnWidths {
                "preset-width-row"
            } else {
                "preset-height-row"
            })
            .flex()
            .items_end()
            .gap(px(8.))
            .h(px(108.))
            .pb(px(2.))
            .overflow_x_scroll()
            .scrollbar_width(px(3.))
            .track_scroll(&self.preset_scroll);
        for (index, size) in values.iter().enumerate() {
            let is_selected = selected.is_some_and(|selection| selection.index == index);
            let (label, value_ratio) = match size {
                PresetSize::Proportion(value) => {
                    (format!("{:.0}%", value * 100.), *value as f32)
                }
                PresetSize::Fixed(value) => (
                    format!("{value}px"),
                    (f64::from(*value) / reference) as f32,
                ),
            };
            let scale = (value_ratio / max_ratio).clamp(0.12, 1.);
            let (width, height) = match list {
                PresetList::ColumnWidths => ((52. + scale * 128.), 58.),
                PresetList::WindowHeights => (58., (24. + scale * 82.)),
            };
            let row_id = format!("preset-row-{list:?}-{index}");
            let click = entity.clone();
            tiles = tiles.child(
                div()
                    .id(SharedString::from(row_id.clone()))
                    .debug_selector(move || row_id.clone())
                    .w(px(width))
                    .h(px(height))
                    .flex_shrink_0()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(if is_selected {
                        p.accent
                    } else {
                        p.control_border
                    })
                    .bg(if is_selected {
                        p.accent.opacity(0.20)
                    } else {
                        p.control
                    })
                    .cursor_pointer()
                    .hover(move |tile| tile.bg(p.hover))
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_click(move |_, _, cx| {
                        click.update(cx, |this, cx| {
                            this.preset_selection = Some(PresetSelection { list, index });
                            cx.notify();
                        });
                    })
                    .child(widgets::label(
                        label,
                        10.5,
                        FontWeight::MEDIUM,
                        if is_selected { p.text } else { p.muted },
                        cx,
                    )),
            );
        }
        let add_id = match list {
            PresetList::ColumnWidths => "preset-width-add-proportion",
            PresetList::WindowHeights => "preset-height-add-proportion",
        };
        let add = entity.clone();
        tiles = tiles.child(
            div()
                .id(add_id)
                .debug_selector(move || add_id.into())
                .w(px(if list == PresetList::ColumnWidths { 44. } else { 58. }))
                .h(px(if list == PresetList::ColumnWidths { 58. } else { 44. }))
                .flex_shrink_0()
                .rounded(px(10.))
                .border_1()
                .border_color(p.control_border)
                .bg(p.control)
                .cursor_pointer()
                .hover(move |tile| tile.bg(p.hover))
                .flex()
                .items_center()
                .justify_center()
                .on_click(move |_, _, cx| {
                    add.update(cx, |this, cx| this.add_preset(list, cx));
                })
                .child(widgets::label(
                    "+",
                    18.,
                    FontWeight::MEDIUM,
                    p.muted,
                    cx,
                )),
        );
        let mut rows = vec![tiles.into_any_element()];
        if let Some(selection) = selected.filter(|selection| selection.index < values.len()) {
            let value = self.selected_preset_value().unwrap_or(50.);
            let change = entity.clone();
            rows.push(
                widgets::slider_row(
                    if list == PresetList::ColumnWidths {
                        "预设宽度"
                    } else {
                        "预设高度"
                    },
                    "preset-value",
                    value,
                    (5., 100.),
                    format!("{value:.0}%"),
                    p,
                    cx,
                    move |value, _, cx| {
                        change.update(cx, |this, cx| this.update_selected_preset(value, cx));
                    },
                )
                .into_any_element(),
            );
            let remove = entity.clone();
            rows.push(
                widgets::row(
                    "操作",
                    widgets::button("preset-remove", "删除", p, cx, move |_, cx| {
                        remove.update(cx, |this, cx| this.remove_selected_preset(cx));
                    }),
                    p,
                    cx,
                )
                .into_any_element(),
            );
            let _ = selection;
        }
        widgets::stack(rows, p).into_any_element()
    }

    fn selected_preset_value(&self) -> Option<f64> {
        let selection = self.preset_selection?;
        let values = self.preset_values(selection.list);
        match values.get(selection.index)? {
            PresetSize::Proportion(value) => Some((value * 100.).clamp(5., 100.)),
            PresetSize::Fixed(value) => {
                let reference = match selection.list {
                    PresetList::ColumnWidths => 1920.,
                    PresetList::WindowHeights => 1080.,
                };
                Some((f64::from(*value) / reference * 100.).clamp(5., 100.))
            }
        }
    }

    fn update_selected_preset(&mut self, value: f64, cx: &mut Context<Self>) {
        let Some(selection) = self.preset_selection else {
            return;
        };
        let mut values = self.preset_values(selection.list);
        if selection.index >= values.len() {
            return;
        }
        values[selection.index] = PresetSize::Proportion((value / 100.).clamp(0.01, 1.));
        self.set_preset_values(selection.list, values, cx);
    }

    fn open_color_picker(&mut self, key: ColorKey, cx: &mut Context<Self>) {
        let text = key.value(&self.commons_draft);
        let color = parse_color_text(&text)
            .unwrap_or_else(|| clarion_config::Color::from_rgba8_unpremul(255, 255, 255, 255));
        let (hue, saturation, value) = rgb_to_hsv(color.r, color.g, color.b);
        self.color_picker = Some(ColorPickerState {
            target: ColorPickerTarget::Common(key),
            hue,
            saturation,
            value,
            alpha: color.a,
        });
        self.color_drag = None;
        cx.notify();
    }

    fn open_accent_picker(&mut self, cx: &mut Context<Self>) {
        let color = parse_color_text(&self.prefs.accent)
            .unwrap_or_else(|| clarion_config::Color::from_rgba8_unpremul(10, 132, 255, 255));
        let (hue, saturation, value) = rgb_to_hsv(color.r, color.g, color.b);
        self.color_picker = Some(ColorPickerState {
            target: ColorPickerTarget::Accent,
            hue,
            saturation,
            value,
            alpha: 1.,
        });
        self.color_drag = None;
        cx.notify();
    }

    fn close_color_picker(&mut self, cx: &mut Context<Self>) {
        if self.color_picker.take().is_some() | self.color_drag.take().is_some() {
            cx.notify();
        }
    }

    fn clear_color_picker(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.color_picker.take() else {
            return;
        };
        self.color_drag = None;
        if let ColorPickerTarget::Common(key) = state.target {
            key.set(&mut self.commons_draft, String::new());
            self.queue_common(cx);
        }
        cx.notify();
    }

    fn commit_color_picker(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.color_picker.take() else {
            return;
        };
        self.color_drag = None;
        match state.target {
            ColorPickerTarget::Common(key) => {
                key.set(
                    &mut self.commons_draft,
                    color_hex(state.hue, state.saturation, state.value, state.alpha),
                );
                self.queue_common(cx);
                cx.notify();
            }
            ColorPickerTarget::Accent => {
                self.prefs.accent =
                    color_hex(state.hue, state.saturation, state.value, 1.);
                self.apply(cx);
            }
        }
    }

    fn set_color_drag(&mut self, drag: ColorDrag, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.color_drag = Some(drag);
        self.update_color_drag(position, cx);
    }

    fn update_color_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.color_drag else {
            return;
        };
        let id = match drag {
            ColorDrag::Sv => "color-sv",
            ColorDrag::Hue => "color-hue",
            ColorDrag::Alpha => "color-alpha",
        };
        let Some(bounds) = self.trigger_bounds.borrow().get(id).copied() else {
            return;
        };
        let x = ((position.x - bounds.left()) / bounds.size.width).clamp(0., 1.);
        let y = ((position.y - bounds.top()) / bounds.size.height).clamp(0., 1.);
        let Some(state) = &mut self.color_picker else {
            return;
        };
        match drag {
            ColorDrag::Sv => {
                state.saturation = x;
                state.value = 1. - y;
            }
            ColorDrag::Hue => state.hue = x,
            ColorDrag::Alpha => state.alpha = x,
        }
        cx.notify();
    }

    fn power_page(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = entity.clone();
        let mut idle = Vec::new();
        idle.push(common_toggle_row(
            "启用空闲策略",
            "idle-enabled",
            self.commons_draft.idle_enabled,
            p,
            cx,
            enabled,
            |commons, value| commons.idle_enabled = value,
        ));
        {
            let change = entity.clone();
            let seconds = self.commons_draft.idle_timeout;
            idle.push(
                widgets::slider_row(
                    "空闲后执行动作",
                    "idle-timeout",
                    f64::from(seconds),
                    (0., 7200.),
                    idle_duration_label(seconds),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.idle_timeout = value.round().clamp(0., 7200.) as u32
                        })
                    },
                )
                .into_any_element(),
            );
        }
        idle.push(
            widgets::row(
                "动作",
                self.dropdown_control(entity, "idle-action", DropdownId::IdleAction, p, cx),
                p,
                cx,
            )
            .into_any_element(),
        );
        idle.push(
            widgets::text_row(
                "锁定命令",
                self.text_inputs[&TextKey::IdleLockCommand].clone(),
                260.,
                p,
                cx,
            )
            .into_any_element(),
        );
        idle.push(common_toggle_row(
            "挂起前锁定",
            "idle-lock-suspend",
            self.commons_draft.idle_lock_before_suspend,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.idle_lock_before_suspend = value,
        ));
        let idle = widgets::stack(idle, p);

        let mut blank = Vec::new();
        {
            let change = entity.clone();
            let seconds = self.commons_draft.idle_blank_timeout;
            blank.push(
                widgets::slider_row_with_help(
                    "无操作后关闭屏幕",
                    "idle-blank",
                    f64::from(seconds),
                    (0., 7200.),
                    idle_duration_label(seconds),
                    Some(self.help_hint(
                        "help-idle-blank",
                        "0 表示不自动熄屏；输入或 inhibit 客户端会重置计时。",
                        p,
                        cx,
                    )),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.idle_blank_timeout = value.round().clamp(0., 7200.) as u32
                        })
                    },
                )
                .into_any_element(),
            );
        }
        let blank = widgets::stack(blank, p);

        div()
            .flex()
            .flex_col()
            .child(widgets::section("空闲与锁定", p, cx, idle))
            .child(widgets::section("屏幕", p, cx, blank))
            .into_any_element()
    }

    fn keyboard_page(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut typing = Vec::new();
        {
            let change = entity.clone();
            typing.push(
                widgets::slider_row(
                    "按键重复速度",
                    "keyboard-rate",
                    f64::from(self.commons_draft.keyboard_repeat_rate),
                    (0., 100.),
                    format!("{} 次/秒", self.commons_draft.keyboard_repeat_rate),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.keyboard_repeat_rate = value.round().clamp(0., 100.) as u8
                        })
                    },
                )
                .into_any_element(),
            );
        }
        {
            let change = entity.clone();
            typing.push(
                widgets::slider_row(
                    "重复前延迟",
                    "keyboard-delay",
                    f64::from(self.commons_draft.keyboard_repeat_delay),
                    (100., 1000.),
                    format!("{} ms", self.commons_draft.keyboard_repeat_delay),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.keyboard_repeat_delay = value.round().clamp(100., 1000.) as u16
                        })
                    },
                )
                .into_any_element(),
            );
        }
        let typing = widgets::stack(typing, p);

        let numlock = widgets::stack(
            vec![common_toggle_row(
                "启动时启用数字锁定",
                "keyboard-numlock",
                self.commons_draft.keyboard_numlock,
                p,
                cx,
                entity.clone(),
                |commons, value| commons.keyboard_numlock = value,
            )],
            p,
        );

        let xkb = widgets::stack(
            vec![
                widgets::text_row(
                    "布局",
                    self.text_inputs[&TextKey::XkbLayout].clone(),
                    220.,
                    p,
                    cx,
                )
                .into_any_element(),
                widgets::text_row(
                    "变体",
                    self.text_inputs[&TextKey::XkbVariant].clone(),
                    220.,
                    p,
                    cx,
                )
                .into_any_element(),
                widgets::text_row(
                    "选项",
                    self.text_inputs[&TextKey::XkbOptions].clone(),
                    320.,
                    p,
                    cx,
                )
                .into_any_element(),
            ],
            p,
        );

        div()
            .flex()
            .flex_col()
            .child(widgets::section("输入体验", p, cx, typing))
            .child(widgets::section("数字键盘", p, cx, numlock))
            .child(widgets::section_with_help(
                "键盘布局（XKB）",
                Some(self.help_hint(
                    "help-xkb",
                    "留空时使用系统 locale；布局可写 us、us,cn 等，选项用逗号分隔。",
                    p,
                    cx,
                )),
                p,
                cx,
                xkb,
            ))
            .into_any_element()
    }

    fn system_page(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let capture = vec![
            widgets::text_row(
                "截图保存路径",
                self.text_inputs[&TextKey::ScreenshotPath].clone(),
                360.,
                p,
                cx,
            )
            .into_any_element(),
            common_toggle_row(
                "禁用剪贴板主选区",
                "clipboard-disable-primary",
                self.commons_draft.clipboard_disable_primary,
                p,
                cx,
                entity.clone(),
                |commons, value| commons.clipboard_disable_primary = value,
            ),
        ];
        let capture = widgets::stack(capture, p);

        let xwayland = vec![
            common_toggle_row(
                "启用 Xwayland Satellite",
                "xwayland-enabled",
                self.commons_draft.xwayland_enabled,
                p,
                cx,
                entity.clone(),
                |commons, value| commons.xwayland_enabled = value,
            ),
            widgets::text_row(
                "可执行文件",
                self.text_inputs[&TextKey::XwaylandPath].clone(),
                260.,
                p,
                cx,
            )
            .into_any_element(),
        ];
        let xwayland = widgets::stack(xwayland, p);

        let decoration = widgets::stack(
            vec![common_toggle_row(
                "客户端装饰优先",
                "prefer-no-csd",
                self.commons_draft.prefer_no_csd,
                p,
                cx,
                entity.clone(),
                |commons, value| commons.prefer_no_csd = value,
            )],
            p,
        );

        let notices = vec![
            common_toggle_row(
                "启动时跳过热键提示",
                "hotkey-skip-startup",
                self.commons_draft.hotkey_skip_at_startup,
                p,
                cx,
                entity.clone(),
                |commons, value| commons.hotkey_skip_at_startup = value,
            ),
            common_toggle_row(
                "隐藏未绑定的快捷键",
                "hotkey-hide-unbound",
                self.commons_draft.hotkey_hide_not_bound,
                p,
                cx,
                entity.clone(),
                |commons, value| commons.hotkey_hide_not_bound = value,
            ),
            common_toggle_row(
                "配置加载失败时不通知",
                "config-notification-off",
                self.commons_draft.config_notification_disable_failed,
                p,
                cx,
                entity.clone(),
                |commons, value| commons.config_notification_disable_failed = value,
            ),
        ];
        let notices = widgets::stack(notices, p);

        div()
            .flex()
            .flex_col()
            .child(widgets::section("截图与剪贴板", p, cx, capture))
            .child(widgets::section("Xwayland", p, cx, xwayland))
            .child(widgets::section("窗口装饰", p, cx, decoration))
            .child(widgets::section("提示与通知", p, cx, notices))
            .into_any_element()
    }

    fn trackpad_page(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut touchpad = Vec::new();
        touchpad.push(common_toggle_row(
            "轻点来点按",
            "touchpad-tap",
            self.commons_draft.touchpad_tap,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.touchpad_tap = value,
        ));
        touchpad.push(common_toggle_row(
            "自然滚动",
            "touchpad-natural",
            self.commons_draft.touchpad_natural_scroll,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.touchpad_natural_scroll = value,
        ));
        touchpad.push(common_toggle_row(
            "打字时防止误触",
            "touchpad-dwt",
            self.commons_draft.touchpad_dwt,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.touchpad_dwt = value,
        ));
        {
            let change = entity.clone();
            touchpad.push(
                widgets::slider_row(
                    "指针速度",
                    "touchpad-speed",
                    self.commons_draft.touchpad_speed,
                    (-1., 1.),
                    format!("{:+.2}", self.commons_draft.touchpad_speed),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.touchpad_speed = value.clamp(-1., 1.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        {
            let change = entity.clone();
            touchpad.push(
                widgets::slider_row(
                    "滚动速度",
                    "touchpad-scroll",
                    self.commons_draft.touchpad_scroll_factor,
                    (0.1, 3.),
                    format!("{:.2} ×", self.commons_draft.touchpad_scroll_factor),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.touchpad_scroll_factor = value.clamp(0.1, 3.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        for (title, id, dropdown) in [
            ("点击方式", "touchpad-click", DropdownId::TouchpadClick),
            ("轻点按钮映射", "touchpad-tap-map", DropdownId::TouchpadTapMap),
            ("加速配置", "touchpad-accel", DropdownId::TouchpadAccel),
            ("滚动方式", "touchpad-scroll", DropdownId::TouchpadScroll),
        ] {
            touchpad.push(
                widgets::row(
                    title,
                    self.dropdown_control(entity, id, dropdown, p, cx),
                    p,
                    cx,
                )
                .into_any_element(),
            );
        }
        for (title, id, value, update) in [
            (
                "拖拽",
                "touchpad-drag",
                self.commons_draft.touchpad_drag,
                (|commons: &mut CommonSettings, value| commons.touchpad_drag = value)
                    as fn(&mut CommonSettings, bool),
            ),
            (
                "拖拽锁定",
                "touchpad-drag-lock",
                self.commons_draft.touchpad_drag_lock,
                |commons, value| commons.touchpad_drag_lock = value,
            ),
            (
                "左右手",
                "touchpad-left-handed",
                self.commons_draft.touchpad_left_handed,
                |commons, value| commons.touchpad_left_handed = value,
            ),
            (
                "中键模拟",
                "touchpad-middle-emulation",
                self.commons_draft.touchpad_middle_emulation,
                |commons, value| commons.touchpad_middle_emulation = value,
            ),
            (
                "外接鼠标时禁用",
                "touchpad-disable-external",
                self.commons_draft.touchpad_disable_on_external_mouse,
                |commons, value| commons.touchpad_disable_on_external_mouse = value,
            ),
            (
                "指点杆输入时防止误触",
                "touchpad-dwtp",
                self.commons_draft.touchpad_dwtp,
                |commons, value| commons.touchpad_dwtp = value,
            ),
        ] {
            touchpad.push(common_toggle_row(
                title,
                id,
                value,
                p,
                cx,
                entity.clone(),
                update,
            ));
        }
        let touchpad = widgets::stack(touchpad, p);

        let mut mouse = Vec::new();
        mouse.push(common_toggle_row(
            "自然滚动",
            "mouse-natural",
            self.commons_draft.mouse_natural_scroll,
            p,
            cx,
            entity.clone(),
            |commons, value| commons.mouse_natural_scroll = value,
        ));
        {
            let change = entity.clone();
            mouse.push(
                widgets::slider_row(
                    "指针速度",
                    "mouse-speed",
                    self.commons_draft.mouse_speed,
                    (-1., 1.),
                    format!("{:+.2}", self.commons_draft.mouse_speed),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.mouse_speed = value.clamp(-1., 1.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        mouse.push(
            widgets::row(
                "加速配置",
                self.dropdown_control(
                    entity,
                    "mouse-accel",
                    DropdownId::MouseAccel,
                    p,
                    cx,
                ),
                p,
                cx,
            )
            .into_any_element(),
        );
        mouse.push(
            widgets::row(
                "滚动方式",
                self.dropdown_control(
                    entity,
                    "mouse-scroll",
                    DropdownId::MouseScroll,
                    p,
                    cx,
                ),
                p,
                cx,
            )
            .into_any_element(),
        );
        for (title, id, value, update) in [
            (
                "左右手",
                "mouse-left-handed",
                self.commons_draft.mouse_left_handed,
                (|commons: &mut CommonSettings, value| commons.mouse_left_handed = value)
                    as fn(&mut CommonSettings, bool),
            ),
            (
                "中键模拟",
                "mouse-middle-emulation",
                self.commons_draft.mouse_middle_emulation,
                |commons, value| commons.mouse_middle_emulation = value,
            ),
        ] {
            mouse.push(common_toggle_row(
                title,
                id,
                value,
                p,
                cx,
                entity.clone(),
                update,
            ));
        }
        let mouse = widgets::stack(mouse, p);

        let mut trackpoint = Vec::new();
        for (title, id, value, update) in [
            (
                "启用指点杆",
                "trackpoint-enabled",
                self.commons_draft.trackpoint_enabled,
                (|commons: &mut CommonSettings, value| commons.trackpoint_enabled = value)
                    as fn(&mut CommonSettings, bool),
            ),
            (
                "自然滚动",
                "trackpoint-natural",
                self.commons_draft.trackpoint_natural_scroll,
                |commons, value| commons.trackpoint_natural_scroll = value,
            ),
            (
                "左右手",
                "trackpoint-left-handed",
                self.commons_draft.trackpoint_left_handed,
                |commons, value| commons.trackpoint_left_handed = value,
            ),
            (
                "中键模拟",
                "trackpoint-middle-emulation",
                self.commons_draft.trackpoint_middle_emulation,
                |commons, value| commons.trackpoint_middle_emulation = value,
            ),
        ] {
            trackpoint.push(common_toggle_row(
                title,
                id,
                value,
                p,
                cx,
                entity.clone(),
                update,
            ));
        }
        {
            let change = entity.clone();
            trackpoint.push(
                widgets::slider_row(
                    "指针速度",
                    "trackpoint-speed",
                    self.commons_draft.trackpoint_speed,
                    (-1., 1.),
                    format!("{:+.2}", self.commons_draft.trackpoint_speed),
                    p,
                    cx,
                    move |value, _, cx| {
                        common_change(&change, cx, |commons| {
                            commons.trackpoint_speed = value.clamp(-1., 1.)
                        })
                    },
                )
                .into_any_element(),
            );
        }
        trackpoint.push(
            widgets::row(
                "加速配置",
                self.dropdown_control(
                    entity,
                    "trackpoint-accel",
                    DropdownId::TrackpointAccel,
                    p,
                    cx,
                ),
                p,
                cx,
            )
            .into_any_element(),
        );
        let trackpoint = widgets::stack(trackpoint, p);

        div()
            .flex()
            .flex_col()
            .child(widgets::section("触控板", p, cx, touchpad))
            .child(widgets::section("鼠标", p, cx, mouse))
            .child(widgets::section("指点杆", p, cx, trackpoint))
            .into_any_element()
    }

    fn menu_bar(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let height_entity = entity.clone();
        let background_entity = entity.clone();
        let autohide_entity = entity.clone();
        let date_entity = entity.clone();
        let seconds_entity = entity.clone();
        div()
            .flex()
            .flex_col()
            .child(widgets::section(
                "高度与背景",
                p,
                cx,
                widgets::stack(
                    vec![
                        widgets::slider_row(
                            "菜单栏高度",
                            "bar-height",
                            self.prefs.bar_height,
                            (22., 48.),
                            format!("{:.0} pt", self.prefs.bar_height),
                            p,
                            cx,
                            move |value, _, cx| {
                                change(&height_entity, cx, |prefs| {
                                    prefs.bar_height = value.clamp(22., 48.)
                                })
                            },
                        )
                        .into_any_element(),
                        toggle_row(
                            "在壁纸上显示一层底色",
                            "bar-background",
                            self.prefs.bar_background,
                            p,
                            cx,
                            background_entity,
                            |prefs, value| prefs.bar_background = value,
                        ),
                        toggle_row(
                            "自动隐藏菜单栏",
                            "bar-autohide",
                            self.prefs.bar_auto_hide,
                            p,
                            cx,
                            autohide_entity,
                            |prefs, value| prefs.bar_auto_hide = value,
                        ),
                    ],
                    p,
                ),
            ))
            .child(widgets::section(
                "时钟",
                p,
                cx,
                widgets::stack(
                    vec![
                        toggle_row(
                            "显示日期",
                            "clock-date",
                            self.prefs.clock_show_date,
                            p,
                            cx,
                            date_entity,
                            |prefs, value| prefs.clock_show_date = value,
                        ),
                        toggle_row(
                            "显示秒",
                            "clock-seconds",
                            self.prefs.clock_show_seconds,
                            p,
                            cx,
                            seconds_entity,
                            |prefs, value| prefs.clock_show_seconds = value,
                        ),
                    ],
                    p,
                ),
            ))
            .into_any_element()
    }

    fn accessibility(
        &mut self,
        entity: &Entity<SettingsApp>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = entity.clone();
        div()
            .flex()
            .flex_col()
            .child(widgets::section(
                "动态效果",
                p,
                cx,
                widgets::stack(
                    vec![toggle_row(
                        "减少动态效果",
                        "reduce-motion",
                        self.prefs.reduce_motion,
                        p,
                        cx,
                        entity,
                        |prefs, value| prefs.reduce_motion = value,
                    )],
                    p,
                ),
            ))
            .into_any_element()
    }


}

fn hot_corner_action_choices(
    corner: &'static str,
) -> Vec<(DropdownChoice, SharedString)> {
    [
        ("overview", "概览"),
        ("expo", "Expo 网格"),
        ("window-spread", "窗口展开"),
        ("launcher", "启动台"),
        ("control-center", "控制中心"),
        ("settings", "设置"),
        ("none", "不执行"),
    ]
    .into_iter()
    .map(|(key, label)| {
        (
            DropdownChoice::HotCornerAction(corner, key),
            label.into(),
        )
    })
    .collect()
}

fn hot_corner_action_label(key: &str) -> String {
    match key {
        "none" => "不执行",
        "expo" => "Expo 网格",
        "window-spread" => "窗口展开",
        "launcher" => "启动台",
        "control-center" => "控制中心",
        "settings" => "设置",
        _ => "概览",
    }
    .into()
}

fn parse_color_text(text: &str) -> Option<clarion_config::Color> {
    clarion_config::Color::from_str(text.trim()).ok()
}

fn color_to_hsla(color: clarion_config::Color) -> gpui::Hsla {
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u32;
    gpui::rgba(
        (channel(color.r) << 24)
            | (channel(color.g) << 16)
            | (channel(color.b) << 8)
            | channel(color.a),
    )
    .into()
}

fn hsla_from_hsv(hue: f32, saturation: f32, value: f32, alpha: f32) -> gpui::Hsla {
    let (r, g, b) = hsv_to_rgb(hue, saturation, value);
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u32;
    gpui::rgba(
        (channel(r) << 24) | (channel(g) << 16) | (channel(b) << 8) | channel(alpha),
    )
    .into()
}

fn color_hex(hue: f32, saturation: f32, value: f32, alpha: f32) -> String {
    let (r, g, b) = hsv_to_rgb(hue, saturation, value);
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    if alpha >= 0.999 {
        format!(
            "#{:02x}{:02x}{:02x}",
            channel(r),
            channel(g),
            channel(b)
        )
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            channel(r),
            channel(g),
            channel(b),
            channel(alpha)
        )
    }
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> (f32, f32, f32) {
    let hue = (hue.fract() + 1.) * 6.;
    let chroma = value * saturation;
    let x = chroma * (1. - (hue % 2. - 1.).abs());
    let (r, g, b) = match hue as u32 % 6 {
        0 => (chroma, x, 0.),
        1 => (x, chroma, 0.),
        2 => (0., chroma, x),
        3 => (0., x, chroma),
        4 => (x, 0., chroma),
        _ => (chroma, 0., x),
    };
    let m = value - chroma;
    (r + m, g + m, b + m)
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0. {
        0.
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.) / 6.
    } else if max == g {
        ((b - r) / delta + 2.) / 6.
    } else {
        ((r - g) / delta + 4.) / 6.
    };
    (hue, if max == 0. { 0. } else { delta / max }, max)
}

fn gtk_config_dir(environment: &HashMap<String, String>) -> Option<PathBuf> {
    environment
        .get("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            environment
                .get("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
}

fn gtk_settings_paths(environment: &HashMap<String, String>) -> Vec<PathBuf> {
    let Some(config) = gtk_config_dir(environment) else {
        return Vec::new();
    };
    vec![
        config.join("gtk-4.0/settings.ini"),
        config.join("gtk-3.0/settings.ini"),
    ]
}

fn read_gtk_setting(environment: &HashMap<String, String>, key: &str) -> Option<String> {
    for path in gtk_settings_paths(environment) {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in text.lines() {
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            if name.trim() == key {
                let value = value.trim().trim_matches('"').trim();
                if !value.is_empty() {
                    return Some(value.to_owned());
                }
            }
        }
    }
    None
}

fn write_gtk_setting(
    environment: &HashMap<String, String>,
    key: &str,
    value: &str,
) -> std::io::Result<()> {
    for path in gtk_settings_paths(environment) {
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let mut lines = Vec::new();
        let mut replaced = false;
        if existing.trim().is_empty() {
            lines.push("[Settings]".to_owned());
        }
        for line in existing.lines() {
            let matches = line
                .split_once('=')
                .is_some_and(|(name, _)| name.trim() == key);
            if matches {
                lines.push(format!("{key}={value}"));
                replaced = true;
            } else {
                lines.push(line.to_owned());
            }
        }
        if !replaced {
            lines.push(format!("{key}={value}"));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, format!("{}\n", lines.join("\n")))?;
    }
    Ok(())
}

fn data_dir_roots(environment: &HashMap<String, String>, leaf: &str) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = environment.get("HOME").filter(|value| !value.is_empty()) {
        let home = PathBuf::from(home);
        roots.push(home.join(".local/share").join(leaf));
        if leaf == "icons" {
            roots.push(home.join(".icons"));
        }
        if leaf == "themes" {
            roots.push(home.join(".themes"));
        }
    }
    let data_dirs = environment
        .get("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .map(|value| value.split(':').map(PathBuf::from).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![PathBuf::from("/usr/local/share"), PathBuf::from("/usr/share")]);
    for dir in data_dirs {
        roots.push(dir.join(leaf));
    }
    roots
}

fn installed_system_themes(
    environment: &HashMap<String, String>,
    current: &str,
) -> Vec<String> {
    let mut themes = BTreeSet::new();
    for root in data_dir_roots(environment, "themes") {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if !path.join("gtk-3.0").is_dir() && !path.join("gtk-4.0").is_dir() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                themes.insert(name.to_owned());
            }
        }
    }
    if !current.is_empty() {
        themes.insert(current.to_owned());
    }
    themes.into_iter().collect()
}

fn theme_families(names: Vec<String>, require_variant: bool) -> Vec<ThemeFamily> {
    let mut map: std::collections::BTreeMap<
        String,
        (Option<String>, Option<String>, Option<String>, Option<String>),
    > = std::collections::BTreeMap::new();
    for name in names {
        let lower = name.to_lowercase();
        let (key, kind) = if let Some(stem) = lower.strip_suffix("-dark") {
            (stem.to_owned(), 2)
        } else if let Some(stem) = lower.strip_suffix("_dark") {
            (stem.to_owned(), 2)
        } else if let Some(stem) = lower.strip_suffix("-light") {
            (stem.to_owned(), 1)
        } else if let Some(stem) = lower.strip_suffix("_light") {
            (stem.to_owned(), 1)
        } else {
            (lower.clone(), 0)
        };
        let entry = map.entry(key).or_default();
        match kind {
            0 => entry.0 = Some(name),
            1 => {
                entry.1 = Some(name.clone());
                if entry.3.is_none() {
                    entry.3 = Some(name[..name.len() - 6].to_owned());
                }
            }
            2 => {
                entry.2 = Some(name.clone());
                if entry.3.is_none() {
                    entry.3 = Some(name[..name.len() - 5].to_owned());
                }
            }
            _ => {}
        }
    }
    let mut families = map
        .into_iter()
        .filter_map(|(key, (base, light, dark, stem))| {
            let has_variant = light.is_some() || dark.is_some();
            if require_variant && !has_variant {
                return None;
            }
            let name = base
                .clone()
                .or(stem)
                .unwrap_or_else(|| key.clone())
                .trim()
                .to_owned();
            let light = light.or(base);
            Some(ThemeFamily { name, light, dark })
        })
        .collect::<Vec<_>>();
    families.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    families.dedup_by(|left, right| left.name.eq_ignore_ascii_case(&right.name));
    families
}

fn family_for(current: &str, families: &[ThemeFamily]) -> String {
    if current.is_empty() {
        return String::new();
    }
    families
        .iter()
        .find(|family| {
            family.name.eq_ignore_ascii_case(current)
                || family
                    .light
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(current))
                || family
                    .dark
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(current))
        })
        .map(|family| family.name.clone())
        .unwrap_or_else(|| current.to_owned())
}

fn installed_icon_themes(environment: &HashMap<String, String>, current: &str) -> Vec<String> {
    let mut themes = BTreeSet::new();
    for root in data_dir_roots(environment, "icons") {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.join("index.theme").is_file() {
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    themes.insert(name.to_owned());
                }
            }
        }
    }
    if !current.is_empty() {
        themes.insert(current.to_owned());
    }
    themes.into_iter().collect()
}

fn installed_cursor_themes(environment: &HashMap<String, String>, current: &str) -> Vec<String> {
    let mut themes = BTreeSet::new();
    for root in data_dir_roots(environment, "icons") {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.join("cursors").is_dir() {
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    themes.insert(name.to_owned());
                }
            }
        }
    }
    if !current.is_empty() {
        themes.insert(current.to_owned());
    }
    themes.into_iter().collect()
}

fn wallpaper_state(environment: &HashMap<String, String>) -> (Option<PathBuf>, Vec<PathBuf>) {
    let Ok(directory) = clarion_preferences::config_directory(environment) else {
        return (None, Vec::new());
    };
    let Ok(bytes) = std::fs::read(directory.join("wallpaper.json")) else {
        return (None, Vec::new());
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return (None, Vec::new());
    };
    let valid = |value: &str| {
        let path = PathBuf::from(value);
        path.is_file().then_some(path)
    };
    let current = json
        .get("path")
        .and_then(serde_json::Value::as_str)
        .and_then(valid);
    let mut history = Vec::new();
    if let Some(current) = &current {
        history.push(current.clone());
    }
    for entry in json
        .get("history")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(path) = entry.as_str().and_then(valid) else {
            continue;
        };
        if history.len() >= 6 {
            break;
        }
        if !history.contains(&path) {
            history.push(path);
        }
    }
    (current.or_else(|| history.first().cloned()), history)
}

struct WallpaperThumb {
    theme: Arc<Image>,
    history: Arc<Image>,
}

fn wallpaper_thumbnails(path: &Path) -> Option<WallpaperThumb> {
    let image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    Some(WallpaperThumb {
        theme: rounded_thumbnail(&image, 360, 216, 24.)?,
        history: rounded_thumbnail(&image, 216, 114, 12.)?,
    })
}

fn rounded_thumbnail(
    image: &image::DynamicImage,
    width: u32,
    height: u32,
    radius: f32,
) -> Option<Arc<Image>> {
    let mut pixels = image
        .resize_to_fill(width, height, image::imageops::FilterType::Triangle)
        .to_rgba8();
    round_thumbnail_corners(&mut pixels, radius);
    let mut bytes = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut bytes, image::ImageFormat::Png)
        .ok()?;
    Some(Arc::new(Image::from_bytes(
        ImageFormat::Png,
        bytes.into_inner(),
    )))
}

fn round_thumbnail_corners(pixels: &mut image::RgbaImage, radius: f32) {
    let width = pixels.width() as f32;
    let height = pixels.height() as f32;
    for y in 0..pixels.height() {
        for x in 0..pixels.width() {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let cx = px.clamp(radius, width - radius);
            let cy = py.clamp(radius, height - radius);
            let distance = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            let coverage = (radius + 0.5 - distance).clamp(0., 1.);
            if coverage < 1. {
                let mut pixel = *pixels.get_pixel(x, y);
                pixel.0[3] = (f32::from(pixel.0[3]) * coverage).round() as u8;
                pixels.put_pixel(x, y, pixel);
            }
        }
    }
}

fn idle_duration_label(seconds: u32) -> String {
    if seconds == 0 {
        return "永不".into();
    }
    if seconds < 60 {
        format!("{seconds} 秒")
    } else {
        // The slider's value column is 44 px wide; keep the label to one line.
        format!("{} 分", seconds / 60)
    }
}

fn display_change(
    entity: &Entity<SettingsApp>,
    cx: &mut App,
    update: impl FnOnce(&mut OutputSettings),
) {
    entity.update(cx, |this, cx| {
        if let Some(page) = &mut this.display {
            update(&mut page.draft);
        }
        this.display_changed(cx);
    });
}

/// Save one output field that affects only a future login (focus-at-startup)
/// without staging the 15-second current-display preview. Hardware-affecting
/// fields still go through `display_change`.
fn display_config_only(
    entity: &Entity<SettingsApp>,
    cx: &mut App,
    update: impl FnOnce(&mut OutputSettings),
) {
    entity.update(cx, |this, cx| {
        let Some(page) = &this.display else {
            return;
        };
        let target = page.target.clone();
        let before = page.baseline.clone();
        let mut after = before.clone();
        update(&mut after);
        if before == after {
            return;
        }
        this.status = Some(("正在保存…".into(), false, Instant::now()));
        let saved = after.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut manager = ConfigManager::new()?;
                    OutputSettings::apply(&mut manager, &target, &before, &after)?;
                    let ipc = Connection::new();
                    manager.save_automatic_with(|action| ipc.perform_action(action))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        if let Some(page) = &mut this.display {
                            page.baseline.focus_at_startup = saved.focus_at_startup;
                            page.draft.focus_at_startup = saved.focus_at_startup;
                        }
                        this.status = Some(("已保存".into(), false, Instant::now()));
                    }
                    Err(error) => {
                        this.status =
                            Some((format!("保存显示设置失败：{error:#}"), true, Instant::now()));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    });
}

fn common_change(
    entity: &Entity<SettingsApp>,
    cx: &mut App,
    update: impl FnOnce(&mut CommonSettings),
) {
    entity.update(cx, |this, cx| {
        update(&mut this.commons_draft);
        this.queue_common(cx);
    });
}

fn common_toggle_row(
    title: &'static str,
    id: &'static str,
    value: bool,
    p: Palette,
    cx: &gpui::App,
    entity: Entity<SettingsApp>,
    update: fn(&mut CommonSettings, bool),
) -> AnyElement {
    widgets::row(
        title,
        widgets::toggle(id, value, p, move |value, _, cx| {
            common_change(&entity, cx, |commons| update(commons, value));
        }),
        p,
        cx,
    )
    .into_any_element()
}

fn toggle_row(
    title: &'static str,
    id: &'static str,
    value: bool,
    p: Palette,
    cx: &gpui::App,
    entity: Entity<SettingsApp>,
    update: fn(&mut Preferences, bool),
) -> AnyElement {
    widgets::row(
        title,
        widgets::toggle(id, value, p, move |value, _, cx| {
            change(&entity, cx, |prefs| update(prefs, value));
        }),
        p,
        cx,
    )
    .into_any_element()
}

fn transform_key(transform: Transform) -> &'static str {
    match transform {
        Transform::Normal => "normal",
        Transform::_90 => "90",
        Transform::_180 => "180",
        Transform::_270 => "270",
        Transform::Flipped => "flipped",
        Transform::Flipped90 => "flipped-90",
        Transform::Flipped180 => "flipped-180",
        Transform::Flipped270 => "flipped-270",
    }
}

fn transform_from_key(key: &str) -> Transform {
    match key {
        "90" => Transform::_90,
        "180" => Transform::_180,
        "270" => Transform::_270,
        "flipped" => Transform::Flipped,
        "flipped-90" => Transform::Flipped90,
        "flipped-180" => Transform::Flipped180,
        "flipped-270" => Transform::Flipped270,
        _ => Transform::Normal,
    }
}

fn transform_label(key: &str) -> &'static str {
    match key {
        "90" => "向左旋转 90°",
        "180" => "倒转 180°",
        "270" => "向右旋转 90°",
        "flipped" => "水平镜像",
        "flipped-90" => "左转 90° / 镜像",
        "flipped-180" => "垂直镜像",
        "flipped-270" => "右转 90° / 镜像",
        _ => "标准",
    }
}

fn vrr_key(mode: VrrMode) -> &'static str {
    match mode {
        VrrMode::Off => "off",
        VrrMode::Always => "always",
        VrrMode::OnDemand => "on-demand",
    }
}

fn vrr_from_key(key: &str) -> VrrMode {
    match key {
        "always" => VrrMode::Always,
        "on-demand" => VrrMode::OnDemand,
        _ => VrrMode::Off,
    }
}

fn vrr_label(key: &str) -> &'static str {
    match key {
        "always" => "始终开启",
        "on-demand" => "按需",
        _ => "关闭",
    }
}

const EDITORS: [(&str, &str); 14] = [
    ("gnome-text-editor", "文本编辑器"),
    ("kate", "Kate"),
    ("gedit", "gedit"),
    ("code", "Visual Studio Code"),
    ("codium", "VSCodium"),
    ("mousepad", "Mousepad"),
    ("pluma", "Pluma"),
    ("xed", "Xed"),
    ("geany", "Geany"),
    ("kwrite", "KWrite"),
    ("featherpad", "FeatherPad"),
    ("leafpad", "Leafpad"),
    ("sublime_text", "Sublime Text"),
    ("zed", "Zed"),
];

fn discover_editors() -> Vec<EditorApp> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let dirs: Vec<std::path::PathBuf> = std::env::split_paths(&path).collect();
    EDITORS
        .iter()
        .filter_map(|(binary, name)| {
            let exec = dirs
                .iter()
                .map(|dir| dir.join(binary))
                .find(|candidate| candidate.is_file())?;
            Some(EditorApp {
                name: (*name).to_owned(),
                exec,
            })
        })
        .collect()
}

fn short_path(path: &str) -> String {
    let Some(home) = std::env::var("HOME").ok().filter(|home| path.starts_with(home)) else {
        return path.to_owned();
    };
    format!("~{}", &path[home.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};

    fn environment(root: &std::path::Path) -> HashMap<String, String> {
        [
            ("HOME".to_owned(), root.to_string_lossy().into_owned()),
            (
                "XDG_CONFIG_HOME".to_owned(),
                root.to_string_lossy().into_owned(),
            ),
        ]
        .into()
    }

    #[test]
    fn theme_families_require_variants_and_resolve_dark_and_light() {
        let names = vec![
            "Adwaita".to_owned(),
            "Adwaita-dark".to_owned(),
            "Plain".to_owned(),
            "Colloid-Light".to_owned(),
            "Colloid-Dark".to_owned(),
            "Papirus".to_owned(),
        ];
        let filtered = theme_families(names.clone(), true);
        let listed = filtered
            .iter()
            .map(|family| family.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(listed, ["Adwaita", "Colloid"]);
        let adwaita = filtered.iter().find(|family| family.name == "Adwaita").unwrap();
        assert_eq!(adwaita.variant(true), "Adwaita-dark");
        assert_eq!(adwaita.variant(false), "Adwaita");
        let colloid = filtered.iter().find(|family| family.name == "Colloid").unwrap();
        assert_eq!(colloid.variant(true), "Colloid-Dark");
        assert_eq!(colloid.variant(false), "Colloid-Light");
        assert_eq!(family_for("Adwaita-dark", &filtered), "Adwaita");
        assert_eq!(family_for("Colloid-Light", &filtered), "Colloid");
        assert_eq!(family_for("Unknown", &filtered), "Unknown");

        let all = theme_families(names, false);
        assert!(all.iter().any(|family| family.name == "Plain"));
        assert!(all.iter().any(|family| family.name == "Papirus"));
    }

    #[test]
    fn gtk_theme_settings_roundtrip_and_theme_lists_include_installed_entries() {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let gtk = root.path().join("gtk-3.0");
        std::fs::create_dir_all(&gtk).unwrap();
        std::fs::write(
            gtk.join("settings.ini"),
            "[Settings]\ngtk-theme-name=OldTheme\ngtk-icon-theme-name=OldIcons\ngtk-cursor-theme-name=OldCursor\n",
        )
        .unwrap();
        assert_eq!(
            read_gtk_setting(&environment, "gtk-theme-name").as_deref(),
            Some("OldTheme")
        );
        write_gtk_setting(&environment, "gtk-theme-name", "NewTheme").unwrap();
        for version in ["gtk-3.0", "gtk-4.0"] {
            let text =
                std::fs::read_to_string(root.path().join(version).join("settings.ini")).unwrap();
            assert!(text.contains("gtk-theme-name=NewTheme"));
        }
        assert_eq!(
            read_gtk_setting(&environment, "gtk-theme-name").as_deref(),
            Some("NewTheme")
        );

        std::fs::create_dir_all(root.path().join(".themes/TestTheme/gtk-3.0")).unwrap();
        std::fs::create_dir_all(root.path().join(".icons/TestIcons")).unwrap();
        std::fs::write(
            root.path().join(".icons/TestIcons/index.theme"),
            "[Icon Theme]\nName=TestIcons\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.path().join(".icons/TestCursor/cursors")).unwrap();
        assert!(installed_system_themes(&environment, "NewTheme").contains(&"TestTheme".into()));
        assert!(installed_icon_themes(&environment, "NewIcons").contains(&"TestIcons".into()));
        assert!(installed_cursor_themes(&environment, "NewCursor").contains(&"TestCursor".into()));
    }

    #[test]
    fn wallpaper_history_prefers_the_current_image_and_decodes_a_thumbnail() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("clarion-shell");
        std::fs::create_dir_all(&config).unwrap();
        let current = root.path().join("current.png");
        let older = root.path().join("older.png");
        for path in [&current, &older] {
            image::RgbImage::from_pixel(16, 9, image::Rgb([20, 80, 100]))
                .save(path)
                .unwrap();
        }
        std::fs::write(
            config.join("wallpaper.json"),
            serde_json::json!({
                "version": 1,
                "path": current,
                "history": [older, current],
            })
            .to_string(),
        )
        .unwrap();
        let (selected, history) = wallpaper_state(&environment(root.path()));
        assert_eq!(selected.as_ref(), Some(&current));
        assert_eq!(history, vec![current.clone(), older]);
        assert!(wallpaper_thumbnails(&current).is_some());
    }

    #[gpui::test]
    fn switching_appearance_switches_theme_and_icon_variants(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        for version in ["gtk-3.0", "gtk-4.0"] {
            let dir = root.path().join(version);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("settings.ini"),
                "[Settings]\ngtk-theme-name=MacTahoe-Dark\ngtk-icon-theme-name=MacTahoe-dark\n",
            )
            .unwrap();
        }
        for name in ["MacTahoe-Dark", "MacTahoe-Light"] {
            std::fs::create_dir_all(root.path().join(".themes").join(name).join("gtk-3.0"))
                .unwrap();
        }
        for name in ["MacTahoe", "MacTahoe-dark", "MacTahoe-light"] {
            let dir = root.path().join(".icons").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("index.theme"), "[Icon Theme]\n").unwrap();
        }
        let (app, visual) = cx.add_window_view(|_window, cx| {
            let mut app = SettingsApp::with_environment(
                cx,
                environment.clone(),
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            );
            app.select(Pane::Appearance, cx);
            app
        });
        visual.run_until_parked();
        let light = visual
            .debug_bounds("theme-light")
            .expect("the light appearance card paints");
        visual.simulate_click(light.center(), Modifiers::default());
        visual.run_until_parked();
        app.read_with(visual, |app, _| assert_eq!(app.prefs.theme, "light"));
        assert_eq!(
            read_gtk_setting(&environment, "gtk-theme-name").as_deref(),
            Some("MacTahoe-Light")
        );
        assert_eq!(
            read_gtk_setting(&environment, "gtk-icon-theme-name").as_deref(),
            Some("MacTahoe-light")
        );

        let dark = visual
            .debug_bounds("theme-dark")
            .expect("the dark appearance card paints");
        visual.simulate_click(dark.center(), Modifiers::default());
        visual.run_until_parked();
        assert_eq!(
            read_gtk_setting(&environment, "gtk-theme-name").as_deref(),
            Some("MacTahoe-Dark")
        );
        assert_eq!(
            read_gtk_setting(&environment, "gtk-icon-theme-name").as_deref(),
            Some("MacTahoe-dark")
        );
    }

    #[gpui::test]
    fn appearance_page_renders_wallpaper_history(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("clarion-shell");
        std::fs::create_dir_all(&config).unwrap();
        let current = root.path().join("current.png");
        image::RgbImage::from_pixel(16, 9, image::Rgb([30, 90, 140]))
            .save(&current)
            .unwrap();
        std::fs::write(
            config.join("wallpaper.json"),
            serde_json::json!({"version": 1, "path": current}).to_string(),
        )
        .unwrap();
        let environment = environment(root.path());
        let (app, visual) = cx.add_window_view(|_window, cx| {
            let mut app = SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            );
            app.select(Pane::Appearance, cx);
            app
        });
        visual.run_until_parked();
        assert!(visual.debug_bounds("wallpaper-history-0").is_some());
        assert!(visual.debug_bounds("wallpaper-change").is_some());
        app.read_with(visual, |app, _| {
            assert!(app.wallpaper_thumbs.contains_key(&current));
        });
    }

    #[gpui::test]
    fn custom_accent_picker_commits_an_opaque_theme_color(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let (app, visual) = cx.add_window_view(|_window, cx| {
            let mut app = SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            );
            app.select(Pane::Appearance, cx);
            app
        });
        visual.run_until_parked();
        let before = app.read_with(visual, |app, _| app.prefs.accent.clone());
        let button = visual
            .debug_bounds("accent-custom")
            .expect("the accent row ends with a custom colour button");
        visual.simulate_click(button.center(), Modifiers::default());
        visual.run_until_parked();
        assert!(visual.debug_bounds("color-picker-card").is_some());
        assert!(
            visual.debug_bounds("color-clear").is_none(),
            "the accent cannot inherit an automatic colour"
        );
        assert!(
            visual.debug_bounds("color-alpha").is_none(),
            "the accent is always opaque"
        );
        let sv = visual.debug_bounds("color-sv").unwrap();
        visual.simulate_click(sv.center(), Modifiers::default());
        let done = visual.debug_bounds("color-done").unwrap();
        visual.simulate_click(done.center(), Modifiers::default());
        visual.run_until_parked();
        app.read_with(visual, |app, _| {
            assert!(app.color_picker.is_none());
            assert_ne!(app.prefs.accent, before);
            assert_eq!(app.prefs.accent.len(), 7);
            assert!(app.prefs.accent.starts_with('#'));
        });
    }

    #[gpui::test]
    fn power_page_renders_idle_controls(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let (app, visual) = cx.add_window_view(|_window, cx| {
            let mut app = SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            );
            app.select(Pane::Power, cx);
            app
        });
        visual.run_until_parked();
        for id in [
            "idle-enabled",
            "slider-idle-timeout",
            "idle-action",
            "idle-lock-suspend",
        ] {
            assert!(
                visual.debug_bounds(id).is_some(),
                "the power page paints {id}"
            );
        }
        app.read_with(visual, |app, _| {
            assert!(app.text_inputs.contains_key(&TextKey::IdleLockCommand));
        });
    }

    #[gpui::test]
    fn preset_editor_adds_a_visual_preset(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let (app, visual) = cx.add_window_view(|_window, cx| {
            let mut app = SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            );
            app.select(Pane::Layout, cx);
            app
        });
        visual.run_until_parked();
        let before = app.read_with(visual, |app, _| {
            parse_preset_sizes(&app.commons_draft.preset_column_widths)
                .unwrap()
                .len()
        });
        let add = visual
            .debug_bounds("preset-width-add-proportion")
            .expect("the preset editor paints its add button");
        visual.simulate_click(add.center(), Modifiers::default());
        visual.run_until_parked();
        let after = app.read_with(visual, |app, _| {
            parse_preset_sizes(&app.commons_draft.preset_column_widths)
                .unwrap()
                .len()
        });
        assert_eq!(after, before + 1);
        assert!(
            visual.debug_bounds("preset-remove").is_some(),
            "the added preset becomes selected and exposes its editor"
        );
    }

    #[gpui::test]
    fn color_picker_edits_a_swatch_without_text_input(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let (app, visual) = cx.add_window_view(|_window, cx| {
            let mut app = SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            );
            app.select(Pane::WindowEffects, cx);
            app.open_color_picker(ColorKey::BorderActive, cx);
            app
        });
        visual.run_until_parked();
        for id in ["color-picker-card", "color-sv", "color-hue", "color-alpha"] {
            assert!(
                visual.debug_bounds(id).is_some(),
                "the colour picker paints {id}"
            );
        }
        let sv = visual.debug_bounds("color-sv").unwrap();
        visual.simulate_click(sv.center(), Modifiers::default());
        visual.run_until_parked();
        app.read_with(visual, |app, _| {
            assert!(app.color_picker.is_some());
        });
        let done = visual.debug_bounds("color-done").unwrap();
        visual.simulate_click(done.center(), Modifiers::default());
        visual.run_until_parked();
        app.read_with(visual, |app, _| {
            assert!(app.color_picker.is_none());
            assert_ne!(app.commons_draft.border_active_color, app.commons.border_active_color);
        });
    }

    #[gpui::test]
    fn decoration_sections_preview_and_gradient_stay_in_sync(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let (app, visual) = cx.add_window_view(|_window, cx| {
            let mut app = SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            );
            app.select(Pane::WindowEffects, cx);
            app
        });
        visual.run_until_parked();
        for id in [
            "decoration-sample-focused",
            "decoration-sample-unfocused",
            "fx-border",
            "fx-focus-ring",
            "fx-border-gradient",
        ] {
            assert!(
                visual.debug_bounds(id).is_some(),
                "the window style page paints {id}"
            );
        }
        app.update(visual, |app, cx| {
            app.commons_draft.border_gradient_enabled = false;
            cx.notify();
        });
        visual.run_until_parked();
        assert!(
            visual.debug_bounds("dial-fx-gradient-angle").is_none(),
            "gradient controls stay collapsed while the active gradient is off"
        );
        app.update(visual, |app, cx| {
            app.commons_draft.border_gradient_enabled = true;
            cx.notify();
        });
        visual.run_until_parked();
        assert!(visual.debug_bounds("dial-fx-gradient-angle").is_some());
        assert!(visual.debug_bounds("dial-fx-gradient-speed").is_some());
    }

    #[gpui::test]
    fn window_effects_preview_stays_above_the_scroll_and_help_hover_shows_a_tooltip(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let (app, visual) = cx.add_window_view(|_window, cx| {
            let mut app = SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            );
            app.select(Pane::WindowEffects, cx);
            app
        });
        visual.run_until_parked();
        let sample = visual
            .debug_bounds("decoration-sample-focused")
            .expect("the pinned preview paints its focused sample");
        let scroll = visual
            .debug_bounds("settings-detail-scroll")
            .expect("the page keeps a scrolling body");
        assert!(
            sample.bottom() <= scroll.top(),
            "the decoration preview sits above the scrolling body"
        );
        let toggle_before = visual
            .debug_bounds("fx-border")
            .expect("the border controls start below the preview");
        app.update(visual, |app, cx| {
            app.detail_scroll.set_offset(point(px(0.), px(-48.)));
            cx.notify();
        });
        visual.run_until_parked();
        let toggle_after = visual
            .debug_bounds("fx-border")
            .expect("the border controls stay laid out while scrolled");
        assert!(
            toggle_after.top() < toggle_before.top(),
            "the control list scrolls under the pinned preview"
        );
        let sample_after = visual
            .debug_bounds("decoration-sample-focused")
            .expect("the preview survives scrolling");
        assert_eq!(sample.top(), sample_after.top());
        let help = visual
            .debug_bounds("help-help-window-preview")
            .expect("the preview carries a help hint");
        visual.simulate_mouse_move(help.center(), None, Modifiers::default());
        visual.run_until_parked();
        assert!(visual.debug_bounds("help-tooltip").is_some());
        app.read_with(visual, |app, _| {
            assert_eq!(
                app.help_hover.map(|(id, _, _)| id),
                Some("help-window-preview")
            );
        });
        visual.simulate_mouse_move(point(px(4.), px(4.)), None, Modifiers::default());
        visual.run_until_parked();
        assert!(visual.debug_bounds("help-tooltip").is_none());
    }

    #[gpui::test]
    fn gradient_dials_edit_angle_and_rotation(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let (app, visual) = cx.add_window_view(|_window, cx| {
            let mut app = SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            );
            app.select(Pane::WindowEffects, cx);
            app.commons_draft.border_gradient_enabled = true;
            app
        });
        visual.run_until_parked();
        let dial = visual
            .debug_bounds("dial-fx-gradient-angle")
            .expect("the angle dial paints while the gradient is enabled");
        visual.simulate_mouse_move(dial.center(), None, Modifiers::default());
        visual.simulate_mouse_down(dial.center(), MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_up(dial.center(), MouseButton::Left, Modifiers::default());
        visual.run_until_parked();
        app.read_with(visual, |app, _| {
            assert_eq!(app.commons_draft.border_gradient_angle, 0.);
        });
    }

    #[gpui::test]
    fn dock_size_segment_updates_the_preference_model(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let (app, visual) = cx.add_window_view(|_window, cx| {
            let mut app = SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            );
            app.select(Pane::Dock, cx);
            app
        });
        visual.run_until_parked();
        let main = visual.windows()[0];
        let trigger = visual
            .debug_bounds("dock-size")
            .expect("the Dock size dropdown is painted");
        visual.simulate_click(trigger.center(), Modifiers::default());
        visual.run_until_parked();
        let menu_handle = visual
            .windows()
            .into_iter()
            .find(|handle| *handle != main)
            .expect("the dropdown opens its own frosted menu window");
        let mut menu = VisualTestContext::from_window(menu_handle, &visual.cx);
        menu.run_until_parked();
        let option = menu
            .debug_bounds("menu-option-2")
            .expect("the large Dock size option is painted");
        menu.simulate_click(option.center(), Modifiers::default());
        menu.run_until_parked();
        visual.run_until_parked();
        app.read_with(&*visual, |app, _| {
            assert_eq!(app.prefs.dock_size, 64.);
            assert!(
                app.status.as_ref().is_some_and(|(_, error, _)| *error),
                "the fake Shell reports the patch as failed"
            );
        });
    }

    #[gpui::test]
    fn dock_size_segment_sends_a_patch_to_the_shell(cx: &mut TestAppContext) {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;

        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let socket = root.path().join("shell.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let shell = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            stream.write_all(b"ok\n").unwrap();
            line
        });
        let (app, visual) = cx.add_window_view(|_window, cx| {
            let mut app = SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at(socket),
            );
            app.select(Pane::Dock, cx);
            app
        });
        visual.run_until_parked();
        let main = visual.windows()[0];
        let trigger = visual
            .debug_bounds("dock-size")
            .expect("the Dock size dropdown is painted");
        visual.simulate_click(trigger.center(), Modifiers::default());
        visual.run_until_parked();
        let menu_handle = visual
            .windows()
            .into_iter()
            .find(|handle| *handle != main)
            .expect("the dropdown opens its own frosted menu window");
        let mut menu = VisualTestContext::from_window(menu_handle, &visual.cx);
        menu.run_until_parked();
        let option = menu
            .debug_bounds("menu-option-2")
            .expect("the large Dock size option is painted");
        menu.simulate_click(option.center(), Modifiers::default());
        menu.run_until_parked();
        visual.run_until_parked();
        let line = shell.join().unwrap();
        let patch: serde_json::Value = serde_json::from_str(
            line.trim().strip_prefix("preferences-patch ").unwrap(),
        )
        .unwrap();
        assert_eq!(patch["dock_size"], 64.);
        app.read_with(&*visual, |app, _| {
            assert_eq!(app.committed.dock_size, 64.);
            assert!(!app.status.as_ref().is_some_and(|(_, error, _)| *error));
        });
    }

    #[gpui::test]
    fn traffic_light_close_is_a_live_button(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let (_app, visual) = cx.add_window_view(|_window, cx| {
            SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            )
        });
        visual.run_until_parked();
        assert_eq!(visual.windows().len(), 1);
        let close = visual
            .debug_bounds("traffic-close")
            .expect("the close control is painted");
        visual.simulate_click(close.center(), Modifiers::default());
        visual.run_until_parked();
        assert!(
            visual.windows().is_empty(),
            "the close control must remove its own window"
        );
    }

    #[gpui::test]
    fn sidebar_search_selects_the_first_matching_pane(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        let (app, visual) = cx.add_window_view(|_window, cx| {
            SettingsApp::with_environment(
                cx,
                environment,
                ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
            )
        });
        visual.run_until_parked();
        let row = visual
            .debug_bounds("nav-程序坞")
            .expect("the Dock row is painted");
        visual.simulate_click(row.center(), Modifiers::default());
        visual.run_until_parked();
        app.read_with(&*visual, |app, _| assert_eq!(app.pane, Pane::Dock));
    }

    #[gpui::test]
    fn compositor_pages_render_their_controls(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let environment = environment(root.path());
        for (pane, markers) in [
            (
                Pane::Layout,
                vec![
                    "layout-stack",
                    "slider-layout-gaps",
                    "layout-default-width",
                    "layout-center",
                ],
            ),
            (
                Pane::WindowEffects,
                vec![
                    "fx-animations",
                    "slider-fx-speed",
                    "fx-open-effect",
                    "fx-blur",
                ],
            ),
            (
                Pane::Keyboard,
                vec!["slider-keyboard-rate", "keyboard-numlock"],
            ),
            (
                Pane::Trackpad,
                vec![
                    "touchpad-tap",
                    "slider-touchpad-speed",
                    "slider-mouse-speed",
                    "touchpad-click",
                    "trackpoint-enabled",
                ],
            ),
            (
                Pane::System,
                vec![
                    "xwayland-enabled",
                    "prefer-no-csd",
                    "hotkey-skip-startup",
                    "config-notification-off",
                ],
            ),
            (
                Pane::MissionControl,
                vec![
                    "mission-spread",
                    "mission-overview",
                    "fx-wobble",
                    "fx-cube",
                    "fx-glide",
                ],
            ),
            (Pane::Displays, vec!["display-refresh"]),
        ] {
            let (_app, visual) = cx.add_window_view(|_window, cx| {
                let mut app = SettingsApp::with_environment(
                    cx,
                    environment.clone(),
                    ShellBridge::at("/nonexistent/clarion-shell.sock".into()),
                );
                app.select(pane, cx);
                app
            });
            visual.run_until_parked();
            for marker in markers {
                assert!(
                    visual.debug_bounds(marker).is_some(),
                    "{pane:?} should paint {marker}"
                );
            }
        }
    }
}
