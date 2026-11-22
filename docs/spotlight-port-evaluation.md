# Clavis Spotlight → Clarion 1:1 移植评估

更新日期：2026-11-04。状态：P1a/P1b/P1c、P2 动作目录与 web、P3 文件/壁纸与模式轨已实现；
计算/时区/汇率工具、富剪贴板持久化与部分 P2 交互仍待办。
参考实现：`~/.config/quickshell/clavis`（只读）。目标：`clarion-shell`（Rust/GPUI）。

## 0. 结论

- **可行，但不是一个小功能。** Clavis Spotlight 本体约 5.8k 行 QML + 0.6k 行非生成 JS +
  2.1k 行生成目录 + 约 1.2k 行配套 Service，折算到 Rust/GPUI **约 8k–11k 行（含测试）**，
  一次铺开需要 25–40 个工程日；按阶段交付更稳。
- **Clarion 已有一半地基**：面板弹窗机制（全屏 catcher + 弹出矩形 + 磨砂区域 + 淡入淡出 +
  高度弹簧 + 点击外部关闭 + reduce-motion）、应用目录（含 keywords/genericName）、
  剪贴板历史（文本/图片/截图，复制/置顶/删除/清空/暂停）、启动台网格与图标、
  Dock 拖放、桌面文件浏览/快速查看、IPC 唤起命令。
- **真正的新后端只有三块**：文件搜索、汇率（需要联网）、富剪贴板（html/文件/二进制）。
  计算器和时区是纯本地实现，成本可控。
- **建议分三期**：P1 核心面板（apps/clipboard/commands + 评分搜索 + 键盘模型）→
  P2 本地数据补齐（剪贴板 details、设置/动作目录、排序/拖拽、web）→
  P3 新后端（files/wallpapers/calc/currency/time）。
- **最大的视觉风险**是 Clavis 的 web 模式按压形变和模式轨动画（GPUI 没有 3D 变换原语，
  只能用缩放/阴影/位移近似），建议放到最后并允许"效果近似、交互一致"。

## 1. Clavis 能力清单

### 1.1 窗口与呈现

- 居中悬浮面板，锚点 y ≈ 屏幕高度 22%，宽度随模式变化：默认 760（canvas 1100）、
  剪贴板 details 1100×600、应用网格 920×560、壁纸 1240×600。
- 开启动画 210ms（scale 0.96→1、y −8→0），关闭 175ms；按 Esc / 点击面板外 / 再次快捷键关闭。
- 磨砂只覆盖搜索栏与结果面板的实际形状；`blurEdgeInset = 2` 避免二值磨砂硬边。
- 结果区高度自适应（结果多时长高、最高 `resultMaxHeight = 440`），模式切换时高度动画。
- 全屏透明 catcher 承接外部点击；面板窗口独占键盘（打开即聚焦输入框，关闭归还焦点）。

### 1.2 输入框与模式轨

- 64px 高搜索框，左侧搜索图标，右侧 4 个模式圆钮（应用/壁纸/剪贴板/文件），
  `Ctrl+0..4` 直达、`Tab`/`Shift+Tab` 在模式间移动、`Ctrl+0` 回到默认搜索。
- 展开模式轨时搜索框横向收缩（`railWidthContraction`，620ms），选中模式有缩放/颜色动画。
- 斜杠草稿以 pill 形式显示在输入框上方（可点 × 取消），支持 `/` 前缀补全。
- web 模式：输入框按下（press depth）、结果区折叠，形成"地址栏"形态；引擎 pill 可切换搜索引擎。
- 输入法组合期（`inputComposing`）不吞按键；`Backspace` 在空查询时回退上一层（工具→模式）。

### 1.3 模式 / 工具 / 覆盖项

| 类型 | 取值 | 说明 |
|---|---|---|
| 模式 | `search`、`apps`、`wallpapers`、`clipboard`、`files`、`commands` | 顶层路由 |
| 工具 | `web`、`settings`、`actions`、`calculator`、`currency`、`time` | 带返回栈的临时模式 |
| 覆盖项 | `appsLayout(list/grid)`、`appsOrder(smart/most-used/recently-used/name)`、`clipboardLayout(default/details)` | 只在对应模式下可用，退出即失效 |

- 会话模型：`Session` 保存 `mode/tool/query/literal/selectionId/overrides/parent/completion/serial`；
  `parent` 构成工具返回栈，`overrides` 构成覆盖栈，`pop()` 依次回退。
- 输入路由：`>` 进入命令模式；`/` 进入斜杠命令；`\/`、`\>` 转义为字面查询；工具内不再解析前缀。

### 1.4 斜杠命令（24 条白名单）

- 模式 6：`/default` `/apps` `/wallpaper(s)` `/clipboard` `/files` `/commands`
- 工具 6：`/search(web)` `/calc` `/fx(currency)` `/time(tz)` `/find-settings` `/actions`
- 动作 4：`/light` `/dark` `/settings` `/map`
- 覆盖 8：`/list` `/grid` `/smart` `/most-used` `/recent` `/name` `/compact` `/detail(s)`
- 未知命令返回 `unknown`；参数只允许出现在工具命令上；不可用命令返回 `scope` 错误。

### 1.5 键盘模型（逐条对齐）

| 按键 | 行为 |
|---|---|
| `↑` / `↓` | 结果选择上/下（按"视觉行"移动，横向分组内左右移动） |
| `←` / `→` | 搜索模式横向分组内移动；壁纸/应用网格按列移动 |
| `Enter` | 执行选中项 |
| `Shift+Enter` | 剪贴板恢复后保持面板打开 |
| `Ctrl+Enter` | 文件：在文件管理器中显示 |
| `Delete` | 删除选中剪贴板条目 |
| `Ctrl+1..4` | 应用/壁纸/剪贴板/文件 |
| `Alt+1..6` | 文件：图片/视频/音乐/文档/文件夹/压缩包筛选（再按一次取消） |
| `Ctrl+K` | web 模式 |
| `Tab` / `Shift+Tab` | 模式轨焦点 |
| `Backspace`（空查询） | 退出工具/覆盖层 |
| `Esc` | 关面板 / 先清空查询 / 再关闭（分级） |

### 1.6 结果面板与各 provider

- 分组：`apps`（横向图标行）→ `settings` → `actions` → `wallpapers`（横向缩略图）→
  `files`/`web` 扩展行（中间有分隔线）。容量预算 6/2/2/4，选中项超预算时替换最后一个可见项，
  保证选中项始终可见但不扩大预算。
- 视觉行打包：横向组按容量折行成"行 + 单元格"，选择/导航按视觉行而非扁平下标计算；
  激活前用 `id + query` 校验，防止上一轮查询的陈旧结果被触发。
- search 模式：空查询不显示结果；apps/设置/动作/壁纸四路聚合，末尾 files/web 扩展行
  （点击把当前查询带进对应模式）。
- apps 模式：网格（148×134 单元格、hover 1.24 缩放）或列表（52px 行）；
  排序 smart（频率+新鲜度）/most-used/recently-used/name；图标可拖到 Dock；右键/回车启动。
- wallpapers 模式：网格（列数随宽度 1–5）、hover 放大预览、当前壁纸角标、点击应用、
  分页 30 条增量加载。
- clipboard 模式：紧凑列表（标题/副标题/类型/时间）与 details 双栏（1100×600，左列表右预览：
  文本行数/字符数/尺寸/修改时间、图片尺寸、文件元信息、文件丢失/读取失败状态）；
  `Enter` 恢复、`Delete` 删除、清空需确认弹窗。
- files 模式：`fd` 搜索（限 50 条）、`open`/`reveal` 两个动作、加载/空/错误/依赖缺失状态。
- commands 模式：24 条命令的可搜索列表，选中即执行同样的斜杠语义。
- tools：calculator（qalc）、currency（ECB 参考汇率 + 缓存 + 过期标记 + 结构化金额/币种编辑）、
  time（IANA 时区 + 日期解析 + 夏令时歧义折叠选择）、web（引擎 URL + xdg 打开）。

### 1.7 偏好（`SpotlightPage`）

应用布局/排序、剪贴板布局、搜索引擎、动画开关等，写入 `UiPreferences`。

### 1.8 体量参考

| 文件 | 行数 | 职责 |
|---|---|---|
| `LauncherWindow.qml` | 1257 | 窗口、路由、激活、按键、开关动画 |
| `SpotlightResultsPanel.qml` | 1108 | 所有结果布局、分页、确认弹窗 |
| `SpotlightSearchBar.qml` | 485 | 输入、模式轨、pill、web 形变 |
| `SpotlightClipboardDetails.qml` | 410 | 剪贴板详情双栏 |
| `SpotlightClipboardProvider.qml` | 393 | 剪贴板 provider + 动作 |
| `SpotlightModeMorphSurface.qml` | 222 | 模式切换形变 |
| `SpotlightSearchProvider.qml` | 201 | 聚合搜索 |
| `SpotlightToolPanel.qml` | 181 | 工具结果与候选 |
| `SpotlightAppGrid.qml` / `SpotlightAppDrag.qml` | 174 / 102 | 应用网格与拖拽 |
| `SpotlightTemplateController.qml` / `SpotlightCurrencyController.qml` / `SpotlightConversionEditor.qml` | 169 / 111 / 230 | 时区/货币结构化输入 |
| 其余 provider（file/wallpaper/app/command）与 `SpotlightSearchTile/SearchResults/SessionController/Style` | 约 600 | |
| `Common/functions/SpotlightLocalSearch.js` / `SpotlightCommands.js` / `SpotlightSession.js` / `SpotlightAppOrder.js` | 180 / 85 / 180 / 88 | 评分、命令表、会话、排序 |
| `Common/generated/SearchCatalog.js` | 2106 | 设置/动作目录（约 137 条，生成） |
| `Services/`（Tool 231、Clipboard 556、FileSearch 264、Search 158、WallpaperService 约 700） | 约 1900 | 后端与数据源 |

外部依赖：`key-cli`（tool/file/clipboard JSON 协议）、`qalc`、`fd`、ECB 汇率、`xdg-mime`/浏览器。

## 2. Clarion 现状

| 能力 | 现状 | 位置 |
|---|---|---|
| 启动台 | 全屏遮罩 + 应用网格（最多 7 列）+ 280px 搜索框 + 窗口标题匹配；无列表模式、无排序、无拖拽、无右键 | `view/macos_launchpad.rs`（348） |
| 应用目录 | `.desktop` 解析出 name/genericName/description/keywords/categories/mime；搜索为"所有词 substring"、按名称排序，无评分 | `clarion-launcher/src/catalog.rs`（1049） |
| 启动 | `gio launch` 路径，无启动历史持久化（只有内存里的 `launches`） | `clarion-launcher/src/launch.rs` |
| 剪贴板 | ext-data-control 客户端，历史 80 条（1MB 文本 / 16MB 图片），文本/PNG/截图；复制/置顶/删除/清空/暂停；仅会话内 | `services/clipboard.rs`（570）、`services/history.rs`（229）、`view.rs::recent` |
| 剪贴板面板 | 右侧 400×600 弹窗，四个过滤标签 + substring 过滤（图片无文本，搜索时被过滤掉）+ 每项按钮 | `view.rs`（约 230 行） |
| 面板机制 | 全屏 layer-shell + `popup` 矩形 + catcher + 磨砂区域 + 淡入淡出 + 高度弹簧 + reduce-motion | `app/panels.rs`、`view.rs` |
| 键盘 | 输入框（含 IME preedit、选区、复制粘贴）；`↑/↓/Enter/Esc` 只服务启动台 | `input.rs`、`view.rs::bind_keys` |
| 壁纸 | 只能通过文件选择器设置单个文件；无图库、无来源目录 | `Action::PickWallpaper`、`desktop.rs` |
| 文件 | 桌面图标网格、批量选择、快速查看（文本/首帧图片）、打开方式、Dock 文件夹浏览（列表/网格/排序） | `desktop.rs`（2639）、`view/dock_folder.rs`（2085） |
| 设置 | 单个 640×520 滚动面板，约 15 个表单行；无搜索、无行锚点 | `view/macos_preferences.rs`（982） |
| 动作 | 合成器 IPC `Action` 169 个变体 + shell `CoreAction`，无人类可读目录 | `clarion-ipc/src/lib.rs`、`app.rs` |
| 唤起 | `clarion-shell-msg spotlight|launcher|clipboard|recent-content`；默认 F4 → `launcher` | `backend.rs`、`resources/default-config.kdl` |
| 图片 | 后台解码 + 缓存（历史缩略图、托盘图标、壁纸） | `view.rs::cache_images`、`desktop.rs` |

## 3. 差距与成本（核心对照表）

估算口径：新增 Rust 代码行数（含同模块测试），不含数据表；风险按"高/中/低"。

### P1 核心面板

| Clavis 能力 | Clarion 现状 | 方案 | 估算 | 风险 |
|---|---|---|---|---|
| 居中悬浮面板、宽度/高度随模式 | 只有全屏启动台和固定尺寸弹窗 | 新增 `Panel::Spotlight`，用 `modal_popup` + `dynamic_size` 列表；先落 780×540 居中（内边距 18、5 列近正方形网格），按模式扩宽留待后续 | 420–600 | 中 |
| 搜索栏 + 4 模式轨 + 斜杠 pill | 输入框已有，无模式轨/pill | 新的 search-bar 视图；`Ctrl+0..4`/`Tab`；pill 显示斜杠草稿 | 380–520 | 中 |
| 结果框架（分组/行/格/选择/滚动/空态） | 启动台只有网格；Dock 文件夹有行/格但独立 | 抽一个结果列表组件：分组标题、行、横向格、分隔线、2D 选择、滚动到选中项 | 520–720 | 中 |
| 评分搜索（前缀/词首/substring/子序列 5000/4000/3000/2000/1000 阶梯） | `catalog.search` 为 all-terms substring，按名排序 | 在 `clarion-launcher` 新增 `search.rs`，逐条移植 `SpotlightLocalSearch`，纯函数 + 单测；启动台同步受益 | 260–360 | 低 |
| 会话路由（模式/工具/覆盖/回退栈/转义/补全） | 无 | 新增 `spotlight/session.rs` + `commands.rs`（24 条命令表） | 300–420 | 低 |
| apps 模式（网格/列表） | 全屏网格可复用 | 抽出启动台网格单元为共享组件；新增列表行 | 220–320 | 低 |
| clipboard 紧凑模式 | 右侧弹窗已有复制/置顶/删除/清空 | 结果行视图 + 保持打开（Shift+Enter）+ 删除键；复用 services 命令 | 240–340 | 低 |
| search 模式聚合（应用 + 窗口 + files/web 扩展行） | 启动台有应用+窗口 | 分组与预算逻辑；窗口结果作为 Clarion 额外分组 | 140–200 | 低 |
| 唤起与绑定 | `spotlight` 目前等同 `launcher` | `spotlight` → 新面板，`launcher` 保留启动台；默认配置建议 `Mod+Space` | 80–120 | 低 |

**P1 小计：约 2.6k–3.6k 行，8–12 工程日。**

验收：`Mod+Space` 在焦点输出打开居中面板；输入评分正确；`Ctrl+1..4`、`/` 命令、
空查询 `Backspace` 回退、`Esc` 分级关闭、点击外部关闭、reduce-motion 生效；
剪贴板可复制/置顶/删除/清空；`cargo test -p clarion-shell` 基线 211 通过并新增单测。

### P2 本地数据补齐

| Clavis 能力 | Clarion 现状 | 方案 | 估算 | 风险 |
|---|---|---|---|---|
| 剪贴板 details 双栏 | 无 | 列表 + 预览（文本统计、图片尺寸、文件元信息、失败态） | 520–760 | 中 |
| 富剪贴板（html/文件/二进制、元数据、持久化） | 仅 text/plain + image/png；仅会话内 | 扩展 `canonical()` 与 Payload（mime、字节数、尺寸、文件列表）；`text/uri-list` 恢复；可选持久化 | 420–650 | 高 |
| 设置目录 + 跳转 | 偏好面板无搜索、无锚点 | 重构偏好为可寻址行（id + 滚动定位），静态目录表（约 60–80 条） | 380–540 | 中 |
| 动作目录（169 个 IPC action → 可搜索） | 只有枚举 | 精选约 60 条（标题/别名/参数/执行映射），从 `clarion_ipc::Action` 派发 | 380–560 | 中 |
| 应用排序（smart/most-used/recent/name） | 无启动历史 | 持久化 `launchCount/lastLaunchedAt`（JSON schema v1），排序函数 | 260–360 | 低 |
| 应用拖到 Dock | Dock 已有文件夹外部拖放先例 | GPUI 跨窗口拖拽 + Dock 插入预览 | 380–560 | 高 |
| web 模式（引擎表 + 打开浏览器） | 无 | 引擎表 + `gio open` URL（放宽 OpenPath 的绝对路径校验） | 140–220 | 低 |
| web 形变动画 | 无 | 缩放/阴影/位移近似（GPUI 无 3D 变换） | 320–520 | 高 |
| commands 模式列表 | 无 | 复用斜杠命令表 | 120–180 | 低 |
| Spotlight 偏好页 | 无 | 布局/排序/引擎/动画开关 | 220–320 | 低 |

**P2 小计：约 2.8k–4.2k 行，9–14 工程日。**

### P3 新后端

| Clavis 能力 | Clarion 现状 | 方案 | 估算 | 风险 |
|---|---|---|---|---|
| files 搜索 | 无 | 方案 A：内建 walker（`ignore` crate，限流/忽略规则）；方案 B：调用 `fd`（与 Clavis 一致） | 480–760 | 中高 |
| wallpapers 图库 | 只能单文件设置 | 约定壁纸目录（或可配置来源）+ 扫描 + 缩略图（复用解码/缓存）+ hover 预览 + 分页 | 520–780 | 中 |
| calculator | 无 | 优先 `qalc`（Clavis 同款），无 qalc 时内置表达式解析回退 | 340–520 | 中 |
| currency | 无 | ECB 日汇率拉取 + 本地缓存 + 过期标记 + 金额/币种结构化输入；**需新增 HTTP 依赖** | 520–800 | 高 |
| time | 无 | `chrono-tz` + 日期解析 + 夏令时歧义折叠 | 340–520 | 中 |
| 工具面板 UI（候选列表、结果、复制、错误态） | 无 | 统一工具结果视图 | 300–460 | 中 |

**P3 小计：约 2.5k–3.8k 行，10–16 工程日。**

### 合计

约 **8k–11k 行**（含测试），**27–42 工程日**。与 Clavis 本体（约 9.7k 行，含 2.1k 生成目录）同量级。

## 4. 复用地基（降低成本的既有实现）

- `modal_popup()`：全屏窗口 + 弹出矩形 + catcher，直接满足 Spotlight 的窗口形态。
- `changed()` 里的 `dynamic_size` 列表：把 `Panel::Spotlight` 加进去即可按内容改高。
- `popup_exit_panel()`：加入 Spotlight 即可获得关闭淡出。
- 磨砂：`set_glass_region` + `blurEdgeInset` 的既有做法；淡出前必须撤销区域（仓库规则）。
- 动效：`dock_motion::{Motion, Elastic}` + `reduce_motion` 分支。
- 图标：`clarion-shell/assets/icons`（lucide）已有 grid/clipboard/calculator 等，需补
  search/image/files/terminal/globe/clock/currency 约 7 个。
- 文本：`menu_text()` 已处理 CJK 字体回退；输入框已有 IME preedit。

## 5. 风险与约束

1. **视觉 1:1 与仓库设计语言冲突**：Clavis 是 Material 3 + 壁纸取色；Clarion 用
   `SurfaceAppearance`、文字 alpha 恒 1、macOS 圆角/字号。建议"信息架构/交互 1:1，
   配色排版按 Clarion"，否则要改共享调色板。
2. **web 形变**：GPUI 没有 3D 变换/内阴影原语，按压形变只能近似（缩放 + 阴影 + 位移）。
3. **网络**：currency 需要引入 HTTP 客户端（workspace 目前零网络依赖），涉及依赖、隐私、
   缓存与离线降级，是唯一"新增信任面"的功能。
4. **文件搜索**：内建 walker 有性能/忽略规则/权限问题；`fd` 方案与 Clavis 一致但增加运行时依赖。
5. **富剪贴板**：ext-data-control 的 mime 矩阵与文件列表恢复是服务层改动，风险高于 UI。
6. **设置跳转**：偏好面板目前是手写布局，没有行 id，需要先重构才能"搜索设置并跳转"。
7. **按键冲突**：`↑/↓/Enter/Esc` 目前只服务启动台；接入 Spotlight 需要把选择模型
   从 `ShellView.selection`（仅应用下标）改为通用结果下标，并保留启动台行为。
8. **多输出**：IPC 唤起不带输出信息，目前只能用 `active_output()`（Clavis 用焦点显示器）；
   若要"指针所在输出"，需要合成器在命令里带输出名。

## 6. 决策（2026-10-25 确认）

1. **范围**：分期落地 P1 → P2 → P3，当前按 P1 推进。
2. **与启动台的关系**：同一入口、两种呈现。新增偏好
   `launcher_presentation: "fullscreen" | "palette"`（默认 `fullscreen`，保持现状）：
   - `clarion-shell-msg launcher`、Dock 启动台按钮、F4 → 按偏好打开 `Panel::Apps`
     或 `Panel::Spotlight`；
   - `clarion-shell-msg spotlight` → 总是打开弹窗（可与全屏启动台并存）；
   - 再次触发同一入口关闭面板。
3. **文件搜索**：内建 walker。复用 `clarion-launcher` 已依赖的 `walkdir`，加忽略规则、
   结果上限、查询代际取消，不依赖外部 `fd`。
4. **汇率**：暂缓；P3 再评估是否引入 HTTP（当前 workspace 零 HTTP 依赖）。
5. **剪贴板**：本地持久化（`$XDG_STATE_HOME/clarion-shell/`，目录 0700、文件 0600、
   原子写、条数与体积上限），并扩展 text/png/富 mime；隐私默认值在 P2 定。
6. **壁纸来源**：P2 做成设置项（默认扫描 `~/Pictures/Wallpapers`）。
7. **视觉**：交互与信息架构对齐 Clavis，观感（透明度、磨砂、字体、行高、强调色）
   按 Clarion 既有视觉系统。
8. **动作目录**：P2 精选 Clarion IPC 实际可执行的条目，不追求 Clavis 的 169 条全量。

### 代码归属

判断标准：**能否不依赖 GPUI/Wayland 单测**。

| 层 | 内容 |
| --- | --- |
| `clarion-launcher`（纯逻辑） | 评分排序 `search.rs`、会话路由 `session.rs`、命令表 `commands.rs`、启动历史 `usage.rs`、文件 walker（P3）、设置/动作目录数据（P2） |
| `clarion-shell`（进程/渲染/系统服务） | `preferences.rs`（偏好 schema 属于 shell）、`Panel`/`Action`/IPC、`view/spotlight.rs` 渲染、`app/panels.rs` 几何、剪贴板持久化、文件搜索 worker、`backend.rs` 外部请求 |

偏好结构留在 shell（`preferences.json` 是 shell 的配置文件）；`clarion-launcher`
不新增 UI 状态，只提供可复用的纯函数。

## 7. 落地文件

```
clarion-launcher/src/search.rs        # 评分阶梯 + AppOrder + ranked_apps（已实现）
clarion-launcher/src/usage.rs         # 启动历史 schema v1、原子写、usage_score（已实现）
clarion-launcher/src/session.rs       # Mode/Tool/Override/Route/回退栈（已实现）
clarion-launcher/src/commands.rs      # 24 条斜杠命令表 + 解析（已实现）
clarion-launcher/src/files.rs         # 内建 walker（已实现 P3）
clarion-shell/src/preferences.rs      # launcher_presentation + 调色板布局/排序偏好（已实现）
clarion-shell/src/view/macos_preferences.rs  # 「启动台：全屏 / 弹窗」分段控件（已实现）
clarion-shell/src/app.rs              # Panel::Spotlight、Action::Launcher、IPC 分流（已实现）
clarion-shell/src/app/panels.rs       # Spotlight placement + popup_exit（已实现）
clarion-shell/src/view/spotlight.rs   # 搜索栏/提示条 + 网格·列表·命令·斜杠补全（已实现 P1b）
clarion-shell/src/view/macos_dock.rs  # Dock 启动台 tile → Action::Launcher（已实现）
clarion-shell/src/services/clipboard.rs / history.rs  # 持久化（P2）
clarion-shell/src/wallpapers.rs                      # 壁纸目录扫描 + 缩略图（已实现 P3）
resources/default-config.kdl          # 可选：Mod+Space → clarion-shell-msg spotlight
```

## 8. 验证（按仓库流程）

1. `cargo test --locked -p clarion-shell`（基线 223 passed / 1 ignored，新增用例必须通过）。
2. 纯逻辑（评分/会话/排序）优先在 `clarion-launcher` 里做单元测试，不依赖 GPUI。
3. `cargo build --locked --release -p clarion-shell` → `python3 scripts/install-shell.py` →
   报 `~/.local/lib/clarion-shell/current/bin/clarion-shell` 的 SHA256 → 用户重启验证。

## 9. 实现进度

- [x] P1a：`clarion-launcher` 评分/会话/命令/历史四个纯逻辑模块（60 tests）。
- [x] P1a：`launcher_presentation` 偏好 + 设置面板「启动台：全屏 / 弹窗」。
- [x] P1a：`Panel::Spotlight`（居中弹窗、磨砂、点击外部关闭）+ 评分结果列表 +
      键盘 `↑↓` / `↵` / `esc`，Dock 与 IPC 入口按偏好分流。
- [x] P1b：会话接线（`Session` 挂在弹窗视图上）、`Ctrl+0/1` 切搜索/应用、`>` 与
      `/commands` 命令列表、斜杠解析与补全、`/list`·`/grid`·`/smart`·… 覆盖项、
      `Backspace` 回退覆盖项、`esc` 先清空/回退再关闭、空查询应用网格 + 启动历史排序。
      模式轨与设置/动作分组结果已在后续批次补齐；工具页仍待办。
      适配：`>` 前缀保留在输入框内（不做文本改写），`/` 后输入部分名称时 `↵` 执行高亮补全
      而不是报错；未接后端的命令不进列表也不可执行。
      右键菜单复用 Dock 菜单内容，渲染在**独立的 `Surface::LauncherMenu` surface** 上
      （`Host.menu` + `panels::launcher_menu_rect`）：卡片贴在点击的图标旁，启动台窗口保留在
      下面、草稿与选中都不丢；`esc` 或点卡片外只关菜单并把键盘交还启动台，下一次 `esc` 才
      关启动台。卡片必须是独立窗口——合成器只能模糊窗口*背后*的内容，画在启动台窗口里就
      没有磨砂，hover 也会穿到下面的图标；启动台则必须保留（早先换成 `Panel::AppMenu`
      会 discard 启动台，菜单关掉后原窗口残留且无法再关闭）。
      几何：弹窗 740 宽 × 0.85 比例高居中（`panels.rs` 按输出高度收缩）、左右内边距 18、上下 10，
      应用网格 5 列、位点 132×110（tile 上下内边距大幅收窄、图标-标签间距 8、图标 72；每行 5 个等宽 slot 铺满内宽保证左右
      对称、纵向间距 10，静置态至少可见三行分类，「N 项」12px、「更多」13px、节间分隔块
      10px，页眉右上角为无背景的 `wine-symbolic` 图标 28px，预留
      「返回主搜索」；tag/更多点击过滤有回归测试
      selecting_a_category_tag_filters_the_grid_tiles）；基准字号
      14、搜索行 22，文本 nowrap + 省略号折叠，分隔线 2px 且两侧各加 3px 间距。
      静置网格按 XDG 分类分区且**每类一行**（最近使用打头，节头右侧「N 项」计数 +
      溢出分类的「更多」按钮，点击即在 tag 栏单选该分类并铺开全部应用）；tag 栏列出
      catalog 实际存在的类别，点击即过滤；圆角 26 的磨砂区域半径与绘制半径一致，
      方角 blur 光斑已修。键盘提示行移除。
      版式对齐 macOS Applications 面板：无框大号搜索行 + 圆形关闭钮 + 分隔线，胶囊行
      夹在两条分隔线之间，结果区上方是「标题 + 右侧键盘提示」的分区行；输入文字与
      占位符按主题取墨色（浅色卡片上是深字，不再是恒白）。
      观感：圆角 26、外框 2px；出现时背景 dim（`p.scrim`，与弹窗同一 ride 淡入淡出）、
      四条 linear_gradient 拼出的向内柔性边框渐变；外侧投影刻意不用——本 gpui 的
      box shadow 不随圆角裁剪，大圆角下会露出方角；正文字重整体上调一档（NORMAL→MEDIUM
      →SEMIBOLD→BOLD）；UI 小图标优先用系统图标主题的 symbolic 变体（`symbolic_name`
      映射 + 既有 `theme_icon` 查找），缺失时回退随包资产。
- [x] P1c：剪贴板模式（紧凑/详情、`Shift+Enter` 保持打开、`Delete` 删除）。
- [x] P2（第一批）：动作目录接入 palette——`commands.rs` 38 条（18 条 Action），含系统
      动作（锁屏/注销/挂起/重启/关机）、shell 面板（截图/通知中心/最近内容/控制中心）、
      设置开关（启动台呈现、壁纸、减少动效、Dock 自动隐藏）；执行走
      `Action::System` → `backend::Command::System`（loginctl/systemctl），测试用
      `capture_backend` 断言派发而不真的执行。排序切换（smart/most-used/recent/name）
      与 commands 列表在 P1b 已完成。
- [x] P2（剩余）：web 页面（引擎表 + 字面 URL，`clarion-launcher/src/web.rs`）与
      动作目录后续批次。
- [ ] P2（剩余）：剪贴板详情/富 mime 持久化、设置行级目录（可寻址+滚动定位）、
      应用拖到 Dock、web 模式形变动画、Spotlight 偏好页。
- [x] P3（文件与壁纸）：内建文件 walker（`clarion-launcher/src/files.rs`，深度/访问数/结果
      上限、名称 > 路径评分）与壁纸图库
      （`clarion-shell/src/wallpapers.rs`，扫描 `~/Pictures/Wallpapers`、`~/Pictures`、
      `~/壁纸` 与当前壁纸目录，后台生成 360×240 PNG 缩略图）。Shell 侧：
      文件搜索 140ms debounce + generation 丢弃旧结果，`Enter` 打开、`Ctrl+Enter`
      在文件夹中显示；副标题保持「类型 · 大小 · 路径」的版式，路径为结果文件的完整
      路径并用 `~` 缩写家目录、过长时由省略号截断；壁纸三列图库点击即应用到所有输出。
      walker 剪枝隐藏目录以及 `node_modules`、`target`、`build`、`dist`、`vendor`、
      `Pods`、`DerivedData`、`__pycache__`、`site-packages`、`venv`、`zig-cache` 等
      工程产物，另有按位置识别的 `~/go/pkg`、`~/go/bin`、`~/snap`、`.cargo/git`、
      `.cargo/registry`；关键字同时匹配文件/文件夹名与完整路径，所以输入文件夹名会
      同时返回该文件夹及其内部文件。
- [x] 模式切换：`Ctrl+1..4` 直达应用/壁纸/剪贴板/文件（页眉不显示按钮，保持搜索行
      简洁），`Tab`/`Shift+Tab` 循环；`Ctrl+0` 回到评分搜索。`QueryChanged` 按当前页面
      触发文件搜索或壁纸扫描，扫过的壁纸不再因输入过滤而重扫。命令列表里的文件与壁纸
      条目使用主题 `folder-symbolic` / `image-round-symbolic`，缺失时回退随包图标。
- [ ] P3（剩余）：计算器、时区、汇率工具。
