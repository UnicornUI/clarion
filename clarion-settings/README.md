# Clarion Settings (GPUI)

Native GPUI application for the Clarion settings surfaces. Its binary and app
identity are `clarion-settings`; it replaces the retired egui application that
previously used the same name.

## Run

```sh
cargo run -p clarion-settings -- --pane dock
cargo run -p clarion-settings -- --pane appearance
```

The Shell opens it from the system menu, the control center and
`clarion-shell-msg preferences`. `clarion-shell-msg` and the Shell command
socket are not required for standalone editing.

## Layout

- 1200 × 860 normal GPUI window, client-drawn rounded frame and traffic lights.
- 340 px frosted sidebar: capsule search, grouped navigation, 44 px rows,
  coloured pane icons.
- Frosted detail header with back/forward history and the current pane title.
- Grouped cards and 50 px rows matching the reference System Settings style.

The window paints a translucent theme tint (about two thirds opaque), and the
sidebar/header/content only add a light wash on top of it; the cards sit
directly on that glass, so the whole window reads as one frosted sheet instead
of separate transparent panels. Outer and panel hairlines flip between a light
edge on dark and a dark edge on light.

## Assets

All static artwork lives outside `src/`:

- `assets/clarion-settings.png` is the 256 px desktop icon. Installers copy it
  to `share/icons/hicolor/256x256/apps/clarion-settings.png` and the desktop
  entry points at it.
- `assets/icons/` holds the pane artwork embedded with `include_bytes!` by
  `src/icons.rs`. The colour SVGs are rasterized once per process with `resvg`
  at the exact device-pixel size (`30 pt × window scale factor`) and cached per
  size, so GPUI's texture atlas never has to downscale them; the monochrome
  `search.svg` keeps using GPUI's asset loader in `main.rs`.

Keeping both groups under `assets/` means packaging only has to reference one
directory, and the embedded paths are explicit relative paths from `src/`.
See `assets/README.md` for the file-to-pane table and artwork provenance.

## Design system

Every page is composed from the same primitives. New panes must reuse them and
must not introduce one-off controls.

### Tokens (`theme.rs`)

- `theme::palette(dark, accent)` owns every colour; pages never hardcode fills.
  `Palette` includes the window tint, chrome wash, cards, controls, chips,
  separators, text/muted and the accent pair.
- Metrics: window 1200 × 860 (min 900 × 680), sidebar 340, header 70, nav capsule
  46 (buttons 38, chevron 24, divider 22), sidebar item 44, setting row 50,
  window radius 26, panel/card radius 16, gutter 10, `TEXT_SCALE` 1.35.
- All text goes through `theme::text` (via `widgets::label`) so Latin and CJK
  runs are shaped with the same font split.

### Layering

1. Window root: translucent `window`/`window_alt` gradient plus the outer 1 px
   `window_border`.
2. Sidebar and detail panel: the same `chrome` wash (10%); the header paints no
   fill at all.
3. Cards: `card` fill, `card_border`, radius 16, soft shadow.
4. The scroll viewport starts below the header, so rows never slide under it.
5. Borders: dark theme uses light hairlines, light theme uses dark-blue
   hairlines (window, panel and card each have their own token).

### Components (`widgets.rs` / `app.rs`)

| Primitive | Use |
|---|---|
| `widgets::section` | group label (indented to the card's text, `pl 16`) + card |
| `widgets::row` | 50 px label/control row |
| `widgets::stack` | rows with hairline dividers |
| `widgets::toggle` | 52 × 30 switch |
| `widgets::button` | compact rounded action button (live pages) |
| `widgets::slider_row` | labelled slider + right-aligned value |
| `widgets::text_row` | labelled single-line text field (XKB, cursor theme, lock command) |
| `widgets::color_swatch` | round colour control opening the HSV/alpha picker |
| `widgets::label` | shaped text; never build `StyledText` in a page |
| `dropdown_control` | every field with more than two options |

Dropdown anatomy:

- The current value is plain text; only the 34 px circle next to it is
  clickable. The circle shows a `chip` tint at rest and `chip_hover` on hover.
- The circle records its bounds with a `canvas` while painting; the menu is
  anchored to that circle's bottom-right and sits 4 px below it.
- The menu is a separate full-screen layer-shell overlay with the rounded card
  outline as its glass region, so the compositor really frosts the settings
  content behind it (see below).
- Add choices by extending `DropdownId` / `DropdownChoice`; do not build an
  inline segmented control for a new field.

Two write paths:

- **Shell preferences** (`preferences.json`): `change` and `toggle_row` update
  `prefs` and call `SettingsApp::apply`, which sends a `preferences-patch` and
  leaves validation/persistence to the Shell.
- **Compositor configuration** (`config.kdl`): `common_change` and
  `common_toggle_row` update `commons_draft` and call `SettingsApp::queue_common`.
  That debounces 350 ms, runs one write at a time on the background executor and
  uses `clarion-settings-core` (lossless KDL edit, validation, atomic write,
  reload request, `CommonSettings::apply`).

Visual selectors (theme thumbnails, accent swatches) stay as direct controls
because they are not value menus. Icons always go through `crate::icons`.

### Adding a pane

1. Add the entry to `nav.rs`; set `implemented: true` only when `page()` has a
   real view.
2. Dispatch it in `SettingsApp::page`.
3. Compose with the primitives above; use a dropdown for any multi-option
   field.
4. Keep `SettingsApp` the only writer and prefer `clarion-settings-core` for
   config work. Add core tests for new config transforms instead of testing the
   view.

## Header

GPUI cannot blur a window's own pixels, so a bar painted inside the window can
never frost rows sliding underneath it. The header therefore paints no extra
fill at all: it is the same glass as the rest of the detail panel, and the
scroll viewport starts below it. That avoids both the darker "cap" and any
colour difference at the top.

## Frosted popups

The compositor blurs what lies *behind* a surface, never the surface's own
pixels, so a menu card painted into the settings window cannot be frosted and a
plain toplevel cannot be positioned reliably on Wayland (the compositor picks
its own floating position). Dropdown menus therefore open a full-screen
transparent layer-shell overlay on the settings window's output, paint only the
card at the trigger's screen position, and report the card's rounded outline as
the window's glass region.

## Architecture

| Module | Responsibility |
|---|---|
| `theme` | palette, metrics, Latin/CJK text shaping |
| `nav` | pane identity, sidebar catalogue, aliases, deep links |
| `bridge` | Shell preference patch socket (Shell stays the only writer) |
| `widgets` | sections, rows, toggles, sliders, dropdown parts, cards |
| `app` | `SettingsApp` state machine, dropdown menu overlay, page composition |
| `input` | single-line GPUI text input/IME from the Shell |
| `clarion-settings-core` | lossless KDL editing, validation, atomic write, `CommonSettings` |

`preferences.json` is owned by the Shell: the app sends `preferences-patch` and
lets the Shell validate, apply and atomically persist. `config.kdl` is owned by
the compositor: the app edits it through `clarion-settings-core`, which keeps
comments and included files intact, validates the merged result, writes
atomically, and asks the compositor to reload.

## Migration status

- Pages: About This Computer (device facts, Shell/desktop state and the
  preferences/config files), appearance (including cursor), Dock, menu bar,
  accessibility, window effects (including decoration colours, shadows, tab
  indicators and insert hints, plus a pinned focus/border preview with hover hints),
  layout (including
  struts/background, preset sizes and hot corners), keyboard (including XKB),
  input devices (trackpad/mouse/pointing stick), mission control (including
  overview style, workspace shadow and window enhancements),
  displays, power/idle, system (screenshot path, Xwayland, CSD preference,
  clipboard and hint/notification switches). The appearance page owns the wallpaper directory,
  recent-wallpaper strip and a live theme preview that draws the current
  wallpaper behind accent-coloured sample windows.
- Mission control reads live workspaces and windows over compositor IPC
  (`Connection::get_windows/get_workspaces`), with focus/float/minimize/close
  actions, window spread and overview shortcuts.
- Displays edits mode, refresh rate, scale, transform, HDR and VRR through the
  compositor's reversible preview: a change stages a 15-second preview and is
  only written after 保留并保存; 恢复 (or the timeout) restores the old output.
- The display arrangement sits at the top of the page: click a screen to select
  it (the sections below then show that output), drag a screen to move it. The
  preview is collision-aware: the dragged screen snaps out of every other
  output, so an overlapping layout is never shown. Release and confirm the new
  layout in the same modal dialog. The arrangement is written with explicit
  positions for every active output.
- Display changes are confirmed in an in-window modal (the frozen-glass dialog
  pattern) instead of an inline row.
- The sidebar ends with 关于本机 (the renamed overview): machine facts collected
  from `/proc`, DMI and `lspci`, the Shell/desktop state, and the preferences
  and config file paths. Each file has an 编辑 button that opens a chooser with
  the detected editors (`gnome-text-editor`, Kate, VS Code, ...) plus the system
  default; choosing one launches it with that file. The old full-config editor
  and the 配置文件 navigation entry are gone.
