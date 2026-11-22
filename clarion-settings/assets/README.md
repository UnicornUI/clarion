# Settings artwork

All files here are embedded into the `clarion-settings` binary or installed as
its desktop icon. None of them are extracted from macOS.

## `clarion-settings.png`

The 256 px application icon used by the desktop entry and by package metadata.
`scripts/install-settings.py`, `scripts/install-session.sh`, the Cargo package
metadata, the Nix derivation and the RPM spec all install it to
`/usr/share/icons/hicolor/256x256/apps/clarion-settings.png`.

## `icons/`

Pane artwork embedded with `include_bytes!` by `src/icons.rs`. The full-colour
SVGs are rasterized once per process with `resvg`, so they keep their own
gradients instead of being tinted like GPUI's monochrome `svg()` elements:

| File | Pane |
| --- | --- |
| `general.svg` | 关于本机 |
| `animation.svg` | 窗口与动效 |
| `appearance.svg` | 外观 |
| `dock.svg` | 程序坞 |
| `menu-bar.svg` | 菜单栏 |
| `accessibility.svg` | 辅助功能 |
| `window-layout.svg` | 窗口布局 |
| `mission-control.svg` | 调度中心 |
| `display.svg` | 显示器 |
| `power.svg` | 电源与锁屏 |
| `keyboard.svg` | 键盘 |
| `track.svg` | 触控板（文件名为 `track.svg`） |
| `system.svg` | 系统 |
| `search.svg` | 侧栏搜索图标（GPUI 单色路径） |

These are project artwork supplied for Clarion, not files extracted from a
macOS installation. `resources/licenses/removed-apple-assets.sha256` documents
the Apple bitmaps removed from this tree for licensing reasons; nothing in this
directory comes from that import path.
