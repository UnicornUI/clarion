# Clarion 开发说明

## 项目结构

Clarion 是 Rust 写的 Wayland compositor + 独立的 macOS 风格桌面 Shell。根包 `clarion`
是合成器（Vulkan/GL/pixman 渲染、DRM/KMS、udev、winit 嵌套后端、HDR/sRGB），
`clarion-shell` 是桌面壳（顶栏、Dock、面板、启动台、通知），两者通过 `clarion-ipc`
通信。整体边界见 [ARCHITECTURE.md](ARCHITECTURE.md)，以该文档为准。

workspace 成员：`clarion-core`、`clarion-motion`、`clarion-timing`、`clarion-scene`、
`clarion-config`、`clarion-preferences`、`clarion-ipc`、`clarion-visual-tests`、
`clarion-settings-core`、`clarion-launcher`、`clarion-settings`、`clarion-shell`。

`third_party/` 下是两个上游库的 vendored fork（被 workspace `exclude`，通过根
`Cargo.toml` 的 `[patch]` 生效）：

- `third_party/gpui`：Zed GPUI 0.2.2，Shell 的 UI/窗口工具库。本地补丁记录在
  `third_party/gpui/CLARION_PATCHES.md`（layer-shell、`set_glass_region`、区域上限等）。
  改动 GPUI 后必须同步更新该文档。
- `third_party/smithay`：Smithay（Wayland compositor 库），pin 在 git rev `4cf0b62`，
  带 clarion 自己的 damage/occlusion 等修复。升级原则是「按上游 diff 逐块搬」，
  不要整包替换。

## 开发与验证流程

Shell 改动后的验证顺序（release 成功后才安装）：

```sh
cargo test --locked -p clarion-shell
cargo build --locked --release -p clarion-shell
python3 scripts/install-shell.py --binary target/release/clarion-shell
sha256sum ~/.local/lib/clarion-shell/current/bin/clarion-shell
```

安装后由用户手动 `systemctl --user restart clarion-shell` 验证。合成器改动用
`cargo build --locked`（整个 workspace）确认。

约定：

- 不运行 `cargo fmt`（仓库有自己的 `rustfmt.toml`，格式化会让 diff 失控）。
- 不在未获明确指示时 commit / push。
- 全量测试基线：`clarion-shell` 223 passed / 1 ignored。
- clippy 有既有基线警告（bin 5 条），不要新增；不要为了消警告去改无关代码。

## Shell 代码地图

- `clarion-shell/src/app.rs`：`Host`（唯一状态机）、`Action`（所有输入的入口）、
  `Panel`（面板种类）、`Host::open()/close()`、面板窗口的创建与输出选择。
- `clarion-shell/src/app/panels.rs`：每个面板的 `placement()`（尺寸、锚点、arrow、
  frost 轮廓）。弹窗宽度/高度、边缘收拢都在这里。
- `clarion-shell/src/view.rs`：`ShellView` 渲染各 `Surface`（`Bar`、`Dock`、`Panel`、
  `Toast`、`DockEdge`）；顶栏、托盘、时钟、状态按钮、面板玻璃区域。
- `clarion-shell/src/view/macos_dock.rs`：Dock 几何、magnifier、拖拽重排、文件夹
  tile、Dock 弹窗（溢出、右键菜单、启动台入口）。
- `clarion-shell/src/view/dock_folder.rs`：文件夹扇形（独立的全屏 panel 窗口，
  `Panel::DockFolder`），卡片布局、层叠、收回动画、磨砂区域。
- `clarion-shell/src/dock_motion.rs`：Dock 的所有动画原语（`Motion`、spring、
  排序/让位），`DockViewMotion` 在其上缓存每帧状态。
- `clarion-shell/src/preferences.rs`：`SurfaceAppearance`（共享透明度/磨砂策略）。
- `clarion-shell/src/view/system_menu.rs`：顶栏最左 Clarion 标记的下拉菜单（关于本机 / 系统设置 /
  最近使用 │ 睡眠 / 重启 / 关机 │ 锁定 / 退出登录）。行表固定，行高与
  `app/panels.rs::system_menu_height()` 同步；图标全部走系统图标主题的 symbolic 变体，
  设置/最近/睡眠沿用已有随包回退，其余行缺失时不画图标（不新增图形资源）。
- `clarion-shell/src/view/about.rs` + `clarion-shell/src/system_info.rs`：关于本机卡片；设备名、
  CPU、内存、系统、内核、磁盘由 `system_info` 在 system worker 启动时读取一次，挂在
  `SystemSnapshot.facts` 上随心跳进入 `ViewState.system`。
- `clarion-shell/src/desktop.rs`：桌面图标网格、右键菜单、重命名/新建/移入废纸篓、壁纸。
  快速查看（`super-y` 与右键「快速查看」）交给 GNOME Sushi（`backend::Command::PreviewFile`
  启动 `org.gnome.NautilusPreviewer.desktop`），shell 不再画自己的大预览面板；打开或预览
  失败通过 `notify-send` 走系统通知，只有通知本身不可用才回退到 shell 的错误横幅。
- `clarion-shell/src/desktop_icons.rs`：随包 PNG 图标（含 PNG 旋转）；`assets/icons/` 是
  随包图标，新增图标要在 `assets/icons/README.md` 补授权说明。
- `clarion-shell/src/icons.rs`：全 Shell 的图标主题入口。主题选择（GTK/KDE 设置、
  `set_dark` 切明暗变体）、freedesktop 名称解析（`resolve`，按主题缓存）、随包回退
  （`asset`/`asset_svg`/`symbolic`）、resvg 光栅化与 PNG/Image 缓存
  （`artwork`/`image`/`trash`）以及主题目录探测都在这里；新图标来源应走这套 API。
- `clarion-shell/src/file_icons.rs`：桌面/Dock/文件夹扇形的**按类型匹配**文件图标。
  `FileKind` 由扩展名（回退 `mime_guess`）判定，图标名走 freedesktop 主题
  （`folder`、`text-x-generic`、`application-pdf`…，MIME 名在主题的 `mimes/` 分类，
  `icons.rs::probe_theme` 必须包含它）；主题的 MIME 图标多为全彩 SVG，而 GPUI 的
  `svg()` 只认单色，所以 `icons` 用 resvg 光栅化成 256px PNG，这里再当图片画。图片/视频/PDF
  优先用 freedesktop 缩略图缓存（`~/.cache/thumbnails/<桶>/<md5(file://uri)>.png`，
  与 Nautilus 共用，校验 `Thumb::MTime`；未命中 30s 后重试），`.svg` 文件直接光栅化
  自身。图片/视频预览走**固定尺寸覆盖**：居中裁成 **4:3**、缩到 256×192、预烘圆角
  （`PREVIEW_ASPECT`/`PREVIEW_RADIUS`），所以无论原始比例都填满同一个卡片框。
  PDF 保留整页比例，居中缩到图标框的 `PAGE_FILL`（0.90），视觉重量与主题类型图标一致，
  不会因为页面撑满而显得过大。所有素材都以 PNG 字节进缓存，按 `(键, 角度)` 旋转（扇形卡片与 Dock 层叠的
  倾斜都走这条路径）。Finder 风格 tag 读 `user.xdg.tags`（逗号分隔），在图标左下画
  最多 3 个色点；`tag_color` 对常见颜色名做中英映射，其余按名字哈希取色。新增图标
  类型只需要补 `kind`/`icon_name` 两处。
- 全屏启动台（`Panel::Apps`，`view/macos_launchpad.rs`）：**行少列多、左右翻页**。
  页面形状由 `launchpad_metrics(可用宽, 可用高)` 决定（5–9 列 × 2–5 行，页高固定），
  每页 `columns × rows`，页数由选择派生（`launchpad_page`，`selection / per_page` 并夹取），
  所以键盘 `↑/↓` 跨页自然翻页；左右边缘是圆形 `launchpad-prev/next` 箭头（不足一页
  不显示，首/末页淡出），底部 `launchpad-page-<i>` 圆点可点击跳页。搜索栏是
  `TextInput::with_capsule`（紧凑字号、无自带边框）外套 44px 高胶囊，前缀放大镜
  （主题 `system-search-symbolic`，回退 `search.svg`）。
- `clarion-shell/src/view/spotlight.rs`：启动台弹窗（`Panel::Spotlight`）的搜索栏与评分
  结果列表；几何在 `panels.rs`（`SPOTLIGHT_WIDTH` 740 × 0.85 高 modal，左右内边距 18、上下 10）。页眉仿 macOS
  Applications 面板：无框大号搜索行（`TextInput::with_bare`，按主题上色，应用图标走
  `software-store-symbolic`）+ 圆形关闭钮 + 分隔线（2px，两侧各多 3px 间距）。
  应用体下方是**类别 tag 栏**（catalog 里实际存在的每个 `AppCategory` 一个 tag，
  点击过滤、再点取消，`palette_category` 状态；`palette_visible_apps` 是过滤后的
  应用列表——网格与列表都必须用它，直接用 `palette_apps` 会让过滤失效），命令体
  保持单枚模式 chip。网格 5 列、位点 132×110（tile 内上下内边距 ~7、图标-标签间距 8）、图标 72：每行渲染 5 个等宽
  slot（`flex_1`，tile 在 slot 内居中，不足 5 个用空 slot 占位），整行铺满弹窗内宽、
  左右对称，纵向间距 10；节头右侧「N 项」12px、「更多」13px，节间分隔块 10px；
  页眉右上角是无背景的 `wine-symbolic` 图标（28px，比左侧图标小一号，预留
  「返回主搜索」）。基准字号 14、搜索行 22，文本一律 nowrap + 省略号折叠。
  静置网格每个分类**只显示一行**（靠左），超过一行的分类在节头加「更多」按钮——
  点击后 tag 栏仅选中该分类并铺开其全部应用（不横向滚动）；键盘提示行已移除；
  键盘选择/滚动按分区平面序（`palette_app_at`/`palette_selection_into_view`），
  有查询词或选中 tag 时退回单一扁平块。
  弹窗圆角 26（`SPOTLIGHT_RADIUS`，
  合成器磨砂区域半径必须与之一致，否则 blur 会以方角探出圆角）、外框 2px 但近乎透明
  （`fg` 9%，让磨砂壁纸透出颜色）；出现时背景 dim（`p.scrim`，随 ride 淡入）+ 边框
  向内的柔性渐变（`popup_rim_wash`，四条 linear_gradient 拼 inner shadow）；
  **不加外侧投影**——本 gpui 的 box shadow 不随圆角裁剪，大圆角下会露出方角。
  UI svg 图标先查系统图标主题的 symbolic 变体
  （`icons::symbolic` 映射 → `icons::resolve`），缺失时回退 `assets/icons/` 随包文件。
- `clarion-launcher/`：无 GPUI/Wayland 依赖的纯逻辑层——`search.rs`（评分阶梯、
  `AppOrder`）、`session.rs`（模式/工具/覆盖/斜杠路由）、`commands.rs`（38 条斜杠/
  动作命令，其中 18 条 `CommandKind::Action`）、`usage.rs`（启动历史）。新逻辑能在这里
  单测就不要写进 shell。
- 动作目录（P2 第一批）：`commands.rs` 里新增系统动作（`system.lock`/`logout`/
  `suspend`/`reboot`/`shutdown`）、shell 面板（截图/通知中心/最近内容/控制中心）与设置
  开关（启动台呈现、壁纸、减少动效、Dock 自动隐藏）。可执行白名单在
  `view/spotlight.rs::action_ready`，动作映射在 `palette_apply`；系统动作走
  `Action::System(SystemAction)` → worker 的 `backend::Command::System`（`loginctl
  lock-session`/`terminate-session`，`systemctl suspend|reboot|poweroff`，超时 10s），
  弹窗随即关闭。测试用 `testing::capture_backend` 换掉真实 worker，绝不会真的关机。

## Shell 设计约束

- 透明度统一走 `SurfaceAppearance`；**文字 alpha 恒为 1.0**，只对背景做透明。
- 磨砂只覆盖「名称标签板」本身（不含图标、层叠图标、reveal 圆钮），形状等于标签板
  自身的旋转圆角矩形。磨砂是合成器的二值区域，不能随透明度淡出，因此淡出前必须先
  撤销区域（扇形收回时 `fan_opacity > 0.15` 才上报）。
- 顶栏是 `LayerShellKeyboardInteractivity::None`：点它不会改变键盘焦点。所以
  `Host::open()` 优先使用 `Action::on_output()` 带来的「发起点击的表面输出」
  （`Host::dispatch_output`），否则回退 `active_output()`。任何 bar/Dock 上的
  新弹窗入口都必须包 `Action::on_output`，否则会在焦点所在的显示器上弹出。
- `Panel::DockFolder` 的扇形是独立全屏面板：Dock tile 在扇形打开时显示折叠控件，
  关闭时通过 `DockViewMotion::folder_fan_root` 检测过渡并复用 `folder_appear`
  播放图标生长，避免图标单帧跳出。
- 面板窗口的 output 决定其尺寸/位置/display；`ShellView.output` 与 `Host.outputs`
  的键同名。
- 启动台有两种呈现（`Preferences.launcher_presentation`：`fullscreen` 默认 / `palette`）。
  **所有启动台入口统一用 `Action::Launcher`**（按偏好选 `Panel::Apps` 或
  `Panel::Spotlight`），`clarion-shell-msg spotlight` 才强制弹窗；Dock tile 入口仍要包
  `Action::on_output`。不要再直接写 `Action::Panel(Panel::Apps)` 当启动台入口。

## 依赖注意事项

- `Cargo.lock` 把 `libc` 钉在 `0.2.189`：`libc >= 0.2.190` 移除了 Linux 的
  `ENOATTR` 别名，而 `xattr 0.2.3`（`gpui` → `gpui_http_client` → `zed-async-tar`
  的强制依赖，用于下载解包）仍在引用它，升级会导致整个 workspace 编译失败。
  等 `zed-async-tar` 升级 `xattr` 后可解除。
- vendored Smithay 的 winit 后端已按上游适配 `winit 0.31.0-beta.3`（`WindowEvent`
  删除 `DragMoved`、若干枚举变 non-exhaustive）。同类依赖升级失败时，先查上游
  Smithay 对应改动并照搬，保持 fork diff 最小。

## 参考实现与后续任务

Clavis（`~/.config/quickshell/clavis`，Quickshell/QML）是 Shell 视觉与交互的参考
实现，**只读，不修改**。Dock 文件夹扇形、操作卡片等都对齐它的做法。

把 Clavis 的 Spotlight 1:1 移植进 Clarion（交互/信息架构对齐，观感按 Clarion）。
评估与分期见 `docs/spotlight-port-evaluation.md`。相关参考：

- `Modules/Launcher/LauncherWindow.qml`、`SpotlightSearchBar.qml`、`SpotlightResultsPanel.qml`、
  `SpotlightSearchProvider.qml`、`SpotlightCommandProvider.qml`、`SpotlightFileProvider.qml`、
  `SpotlightClipboardProvider.qml`、`SpotlightAppProvider.qml`、`SpotlightWallpaperProvider.qml`、
  `SpotlightToolPanel.qml`、`SpotlightModeMorphSurface.qml`、`SpotlightStyle.qml`
- `Common/functions/SpotlightCommands.js`、`SpotlightLocalSearch.js`、
  `Common/generated/SearchCatalog.js`
- `Services/` 下的 FileSearch/Clipboard/Application 等 provider 服务

进度：P1a 已完成（纯逻辑四模块、`launcher_presentation` 设置项、`Panel::Spotlight`
弹窗 + 评分列表 + 键盘导航、Dock/IPC 分流）。后续 P1b（模式轨、命令列表、分组结果）、
P1c（剪贴板模式）、P2（持久化/目录/排序）、P3（文件 walker、工具）。

## 当前分支状态（2026-10-25）

- 分支 `dock/folder-pin`，比 `origin/dock/folder-pin` 领先 7 个提交（含合并 main、
  `libc` 钉版、winit beta.3 适配），尚未 push。
- 最近完成：Dock 文件夹扇形精修（实底标签板、切线层叠、clavis 式尾部操作卡片）、
  收起磨砂闪帧、图标生长入场、托盘/bar 弹窗按点击显示器归属。
- Spotlight P1a/P1b（未提交）：`clarion-launcher` 新增 `search/session/commands/usage`
  （60 tests，新增 `serde`/`serde_json` 依赖）；shell 新增 `launcher_presentation`、
  `spotlight_app_layout`、`spotlight_app_order` 偏好与设置分段控件、`Panel::Spotlight`、
  `Action::Launcher`、`view/spotlight.rs`（应用网格/列表、命令列表、斜杠补全、覆盖项、
  `Backspace`/`esc` 回退、右键菜单——菜单是独立的 `Surface::LauncherMenu` 全屏磨砂
  surface（启动台窗口保留在下面，`Host.menu` 单独持有；只有独立窗口才能让合成器模糊
  卡片背后的启动台内容，也彻底隔离 hover/点击），`esc`/点外部先关菜单再关启动台）；
  启动历史由 worker 落盘；测试基线 223 passed / 1 ignored。

## 系统设置（GPUI，`clarion-settings`）

- 唯一设置应用；共享逻辑在 `clarion-settings-core`（无损 KDL 编辑、校验、原子写入、
  重载请求、`CommonSettings`），Shell 偏好模型在 `clarion-preferences`（Shell 与设置
  应用共享，避免互相依赖）。
- 视觉 token 在 `clarion-settings/src/theme.rs`；尺寸表：窗口 1200×860、
  侧栏 340、顶栏 70、导航胶囊 46/按钮 38、行高 50、窗口圆角 26、面板/卡片 16。
- 分层：窗口半透明主题底色（约 2/3 不透明）→ 侧栏/详情 10% chrome 洗色 →
  卡片；顶栏不画填充，滚动视口从顶栏下方开始，保证顶部没有色差。
- 边框随主题反转：暗色用浅色发丝线，浅色用深蓝发丝线。
- 组件复用：只用 `widgets::section/row/stack/toggle/slider_row/text_row/color_swatch/button/label`；多选项
  一律 `dropdown_control`（值 + 圆形 chip，仅圆形可点；菜单是独立全屏
  layer-shell overlay，卡片轮廓作为 glass region，距按钮 4px）。不要新增
  内联分段控件或一次性按钮。
- 两条写入路径：Shell 偏好用 `change`/`toggle_row` → `preferences-patch`；
  合成器配置用 `common_change`/`common_toggle_row` → 350ms 防抖后台
  `apply_common_settings`（一次只写一份）。
- 新增页面：在 `nav.rs` 登记（有真实页面才设 `implemented: true`），在
  `SettingsApp::page` 分发；配置逻辑优先放进 `clarion-settings-core` 并补测试。
- 静态资源统一在 `clarion-settings/assets/`：`clarion-settings.png` 供桌面入口与
  各打包脚本安装，`icons/` 由 `src/icons.rs` / `main.rs` 用 `include_bytes!` 嵌入；
  不要在 `src/` 下新增资源目录。
- 外观页含壁纸模块：`preferences.wallpaper_dir` 决定壁纸库与 Spotlight 图片模式的唯一
  搜索目录（空值回退传统图片目录），最近使用的壁纸来自 Shell `wallpaper.json` 的
  `history`，缩略图在后台解码；主题预览把当前壁纸铺进浅色/深色卡片，示例窗口的侧栏
  与底部胶囊使用主题强调色；强调色列表末尾的自定义色按钮复用颜色选择器，只提交
  不透明的 `#rrggbb`。主题区还列出 GTK 系统主题、图标主题与鼠标主题（按 Dark/Light
  家族分组，Light 缺省时用基础名；切换浅色/深色会同步变体）：主题/图标写入
  `~/.config/gtk-{3,4}.0/settings.ini`，图标切换后通过 Shell 的 `reload-icons` 命令
  立即重载；鼠标主题同时写入 Clarion 配置。下拉菜单最多 200px 宽、按内容收缩、长列表
  可滚动，左侧固定勾选列。设置应用通过 `set-wallpaper <json>` 命令让 Shell 应用历史壁纸；
  目录选择走 GPUI 的 `prompt_for_paths`。
- 已迁移：关于本机（原概览）、外观、程序坞、菜单栏、辅助功能、窗口与动效、窗口布局、
  键盘、输入设备（触控板/鼠标/指点杆的高级项）、调度中心、显示器（分辨率/刷新率/缩放/
  方向/HDR/VRR/色深，走 15 秒输出预览确认）、电源与锁屏（idle 空闲/熄屏/锁定）、
  系统（截图路径、Xwayland、客户端装饰、剪贴板主选区、热键提示与配置通知）。
  窗口细节位于页面顶部：边框/光环预览固定在页头，下方列表滚动时预览仍按当前参数
  实时重绘；提示文本统一为 `help-hint-symbolic` 形状的上角标，hover 才显示 tooltip。
  边框含边框/光环/阴影颜色与阴影参数、标签指示器、插入提示、活动渐变（起止色合并为
  一行色块 + 渐变条，角度/旋转合并为两个圆环拨盘；关闭渐变会写回纯色，渐变记忆保存
  在边框旁的注释中）；布局页含 struts、非活动窗口淡化、工作区/全屏背景色、
  圆角柔和度、可视化预设列宽/窗高（比例/固定像素，可点选编辑、增删）；调度中心含
  概览风格/缩放/背景色、工作区阴影与窗口增强（弹性窗口/立方体切换/窗口立体展开），
  不再列出实时工作区/窗口清单。
- 窗口布局页含触发角：`gestures.hot-corners` 的四个开关，以及每角可选的
  `top-left-action`/`top-right-action`/`bottom-left-action`/`bottom-right-action`
  （overview、expo、window-spread、launcher、control-center、settings、none；
  缺省保持旧行为）。合成器需同步重建后才会识别这些动作节点。
- 颜色字段用 `color_swatch` 圆形控件 + 窗内 HSV/透明度弹窗（可点选、可拖动，不提供手输颜色）；
  新增配置页优先复用 `text_row`（主题、XKB、锁定命令）和现有 slider/dropdown/toggle；
  配置读取是本地小文件同步完成，输出/设备能力等慢数据仍走既有异步加载与骨架。
