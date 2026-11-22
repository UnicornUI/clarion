# 系统菜单（左上角图标下拉）规划

状态：v1 已实现（`Panel::System` / `Panel::About`，见 `view/system_menu.rs`、`view/about.rs`）
参考：macOS Apple 菜单截图（`关于本机 / 系统设置 / App Store / 最近使用 / 强制退出 / 睡眠 / 重启 / 关机 / 锁定屏幕 / 退出登录`）

## 1. 目标

顶栏最左侧的 Clarion 标记从「点击直接打开设置面板」改为一个普通图标：

- 单击在图标下方打开系统菜单，再次单击关闭。
- 菜单与现有顶栏弹窗（托盘菜单、控制中心、通知中心）共用一套视觉与窗口机制。
- 首版必须包含四项：**关于本机、重新启动…、关机…、退出登录…**。
- 复用已有的系统动作与面板，不新增权限、不直接操作 D-Bus。

非目标（首版不做）：

- App Store（Clarion 没有应用商店）。
- 「强制退出…」窗口选择器（需要新的运行窗口列表与关闭流程）。
- 「最近使用的项目」二级子菜单（先直接打开现有「最近内容」面板）。
- 菜单项的 ⌃⌘Q / ⇧⌘Q 等全局快捷键。
- 键盘上下键在菜单内移动高亮（Esc、点击外部可用；后续再加）。

## 2. 信息架构

| 区块 | 行 | 图标（主题 symbolic → 回退资源） | 动作 |
| --- | --- | --- | --- |
| 1 | 关于本机 | `laptop-symbolic`（主题） | `Action::Panel(Panel::About)` |
| 1 | 系统设置… | `preferences-system-symbolic` → `settings.svg` | `Action::Panel(Panel::Preferences)` |
| 1 | 最近使用的项目 | `document-open-recent-symbolic` → `menu-folder-open.svg` | `Action::Panel(Panel::Recent)` |
| 2 | 睡眠 | `system-suspend-symbolic` → `moon.svg` | `Action::System(SystemAction::Suspend)` |
| 2 | 重新启动… | `system-reboot-symbolic`（主题） | `Action::System(SystemAction::Reboot)` |
| 2 | 关机… | `system-shutdown-symbolic`（主题） | `Action::System(SystemAction::Shutdown)` |
| 3 | 锁定屏幕 | `system-lock-screen-symbolic`（主题） | `Action::System(SystemAction::Lock)` |
| 3 | 退出登录 “{USER}”… | `system-log-out-symbolic`（主题） | `Action::System(SystemAction::Logout)` |

- 区块之间用 1px 分隔线（与托盘菜单一致：`h(1).my_2().bg(p.line)`）。
- 所有行使用同一套 `row_button` 字体与字重。参考图的首行是粗体，但 Clarion 的 CJK 字体没有对应 semibold face，强行加粗会回退到另一套字体，所以首行保持常规字重，只靠图标区分。
- 「退出登录」标签带用户名：`ViewState` 增加 `user_name`（取启动环境里的 `USER`，回退 `LOGNAME`，再回退「当前用户」）。
- 「睡眠 / 锁定屏幕」是截图里已有、shell 也已支持的动作，首版一并提供；若不想要可只保留四项核心项，其余行的动作已经存在，删除不影响结构。

## 3. 视觉规格

全部复用现有 token，不引入新颜色：

| 项目 | 取值 | 来源 |
| --- | --- | --- |
| 弹窗宽度 | 260 | 与 `Panel::Tray` 相同 |
| 水平位置 | 左边缘 8px | 标记本身在 bar 左内边距 10px 附近 |
| 垂直位置 | `bar_height + 8` | 与托盘/控制中心同一 `top` |
| 圆角 | 12 | 现有 `menu` 面板 |
| 玻璃/磨砂 | 自动 | `modal` 面板的 `popup_region`，半径 12 |
| 背景/描边 | `p.panel` / `p.line` | 与托盘菜单一致 |
| 内边距 | 12 | 托盘菜单同款 `p(12)` |
| 行高 | 26（`px(8) py(4)` + 18px 行高） | `row_button` |
| 行圆角/悬停 | 7 / `p.line` | `row_button` |
| 图标 | 16px | `menu_item_icon` / `named_icon` |
| 字号/行高 | 15 / 18 | `row_button` |
| 行间距 | 6 | `TOP_GAP` |
| 分隔线槽位 | 17 | `TRAY_MENU_SEPARATOR` |

菜单高度按现有公式计算：`行数×26 + 分隔线×17 + 行间 6×(项数-1) + 28`。本方案（8 行 + 2 分隔线）约 324px，远低于 0.8×输出高度的上限。

图标全部走系统图标主题的 symbolic 变体（与 Spotlight 命令行同一套 `theme_icon` 解析），不再向仓库新增图形资源。设置/最近/睡眠沿用项目已有的 `settings.svg`、`menu-folder-open.svg`、`moon.svg` 作为主题缺失时的回退；关于/重启/关机/锁定/退出登录只依赖主题，缺失时该行不画图标。

## 4. 窗口与交互

复用面板机制，不新建独立窗口类型：

- 新增 `Panel::System`。
- 标记的 `on_click` 改为 `Action::on_output(bar_output, Action::AnchoredPanel(Panel::System, x))`：
  - `AnchoredPanel` 已实现「同一个面板再点一次就关闭」。
  - `on_output` 保证菜单出现在点击所在的显示器，而不是键盘焦点所在的显示器。
- 面板是 `modal`：整输出透明 catcher 负责点击外部关闭；`Esc` 走现有 `Dismiss → Action::Close`。
- 打开菜单时给标记加一层 `p.line` 背景（`panel == Some(Panel::System)` 且 `panel_output == self.output`），提示图标处于打开状态。
- 菜单行的点击都走 `Host::handle`：`Action::System` 会在 `Host` 里关闭面板并把命令交给 backend worker；`Panel::About/Preferences/Recent` 会由 `Host::open` 替换当前面板。
- 预览模式（`--preview`）：沿用 backend 的预览保护，系统动作不会真正执行。

## 5. 关于本机

新增 `Panel::About`：居中的竖长 modal 卡片（360×612，圆角 16，磨砂），布局对齐参考图：

```
● ○ ○                     ← 红/黄/绿交通灯（红点关闭）
        [笔记本插画]       ← 深色边框 + 蓝色屏幕 + 底座
          Blade            ← DMI product_name
         archlinux         ← hostname
     芯片    <CPU model>
     显卡    <GPU name>
     内存    <GiB>
   启动磁盘   <可用> / <总量>
     序列号   <DMI serial，读不到则整行隐藏>
     系统    <PRETTY_NAME>
     内核    <osrelease>
      [更多信息…]          ← 打开设置面板
     Clarion 桌面环境
  © 2026 Clarion · GPL-3.0-or-later
```

菜单行「关于本机」的图标使用主题的 `laptop-symbolic`。

数据来源全部由 backend 的 `system_worker` 在启动时读一次（不轮询、不占 UI 线程）：
| 字段 | 来源 |
| --- | --- |
| model | `/sys/devices/virtual/dmi/id/product_name` → `product_version` → hostname |
| hostname | `/etc/hostname` → `HOSTNAME` 环境变量 |
| os_name | `/etc/os-release` 的 `PRETTY_NAME` → `NAME` |
| kernel | `/proc/sys/kernel/osrelease` |
| cpu | `/proc/cpuinfo` 第一个 `model name` → `std::env::consts::ARCH` |
| gpu | `/proc/driver/nvidia/gpus/*/information` 的 `Model` → `lspci -mm` 的 VGA/3D 控制器（NVIDIA/AMD 优先）→ `/sys/class/drm/card*/device/uevent` 的 `DRIVER` + `PCI_ID` |
| serial | `/sys/devices/virtual/dmi/id/product_serial`（不可读时留空） |
| memory_total | `/proc/meminfo` 的 `MemTotal` |
| disk_total / disk_free | `libc::statvfs("/")` 的 `f_blocks×f_frsize`、`f_bavail×f_frsize` |

实现方式：扩展 `backend::SystemSnapshot`（每秒心跳已随 `ViewState.system` 进入 UI），静态字段只填一次；解析函数做成纯函数并单测（`os-release`、`meminfo`、`cpuinfo`、字节格式化）。读不到的字段在卡片里整行隐藏，不显示占位符。

旧 Flutter 版本的「显示器 / 储存空间」分页没有迁移，磁盘信息以启动磁盘一行摘要呈现。

## 6. 代码改动清单

| 文件 | 改动 |
| --- | --- |
| `clarion-shell/src/app.rs` | `Panel::System` / `Panel::About`；`ViewState.user_name`；标记点击动作 |
| `clarion-shell/src/app/panels.rs` | `Panel::System` 的 `modal_popup(w,h,8,top,260,height)`、`Panel::About` 居中卡片；`system_menu_height()` 常量/函数 |
| `clarion-shell/src/view.rs` | 注册新模块；`Panel::System`/`Panel::About` 分派；`modal`/`menu` 匹配加入新面板；`p(12)` 分组 |
| `clarion-shell/src/view/system_menu.rs`（新） | 菜单行表、行渲染、图标回退；`pub(super) fn system_menu(...)` |
| `clarion-shell/src/view/about.rs`（新） | 关于卡片渲染与两列字段布局 |
| `clarion-shell/src/backend.rs` | `SystemSnapshot` 新字段与一次性采集 |
| `clarion-shell/src/system_info.rs`（新） | 静态系统信息采集与解析纯函数 |
| `AGENTS.md` | 代码地图补 `view/system_menu.rs` / `view/about.rs` |

菜单高度与渲染必须用同一份行表（与 `tray_menu_height` 对 `tray_menu` 的做法一致），避免出现底部空白或最后一行被裁掉。

## 7. 测试计划

1. `panels.rs` 单测：`Panel::System` 的左边缘 8px、宽度 260、顶部 `bar+8`、高度等于行表高度；`Panel::About` 水平垂直居中。
2. UI 测试（`cx.add_window_view` + debug selector）：
   - 点击 `clarion-menu-mark` 打开 `Panel::System`；再点一次关闭。
   - 8 行与 3 条分隔线都画出，最后一行底边不超过弹窗底边。
   - 点击「关于本机」→ `Panel::About`，显示版本与系统字段；点击「系统设置…」→ `Panel::Preferences`。
   - `capture_backend` 后点击「关机…」只发出 `backend::Command::System(SystemAction::Shutdown)`，不真正执行；「退出登录」同理。
   - `Esc` 和点击弹窗外部都能关闭。
3. `system_info` 纯函数单测：带引号/转义的 `os-release`、`MemTotal`、`model name` 缺失时回退架构、`statvfs` 字节格式化。
4. 预览模式：菜单可打开，系统动作被 backend 拒绝，不产生副作用。
5. 视觉回归：菜单与托盘菜单并排截图比对圆角、行高、分隔线、磨砂区域。

## 8. 实施顺序

1. `Panel::System` + 行表 + 渲染 + 标记点击切换（可用四项核心动作）。
2. 补齐睡眠/锁定屏幕/系统设置/最近内容行与图标回退；标题首行加粗；用户名校验。
3. `Panel::About` + `system_info` 采集与卡片。
4. 更新 `AGENTS.md` 的代码地图。
5. 单元/UI 测试、视觉比对。

预估：菜单 1–1.5 天，关于本机 0.5–1 天，图标与打磨 0.5 天。

## 9. 已确认的决策

1. 首版采用**完整版**菜单：关于本机 / 系统设置… / 最近使用的项目 │ 睡眠 / 重新启动… / 关机… │ 锁定屏幕 / 退出登录 “用户”…。
2. 「关于本机」使用**自绘 About 卡片**（版本 + 设备/CPU/内存/系统/内核/磁盘），数据由 backend 启动时读取。
3. 关机 / 重启 / 退出登录**直接执行**，与现有 Spotlight 命令一致，不增加二次确认。
