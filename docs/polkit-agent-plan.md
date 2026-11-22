# Clarion 内置 polkit 认证代理规划

## 目标

让 Clarion Shell 直接实现
`/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1` 的职责：

- 作为当前用户 session 的 polkit 认证代理注册到系统总线；
- 在 shell 内显示原生权限确认对话框；
- 通过系统自带的 `polkit-agent-helper-1` 完成 PAM 认证；
- 不再依赖 polkit-gnome / GNOME Shell 等的认证代理。

不在本功能范围内：

- 自己实现 PAM 密码校验或编写 setuid 程序；
- 修改系统 polkit 规则、动作或 `/usr/share/polkit-1/actions`；
- 代替 `pkexec` / polkitd 做授权判断。

## 一、polkit 认证调用链

```
应用 / systemctl / pkexec
        │ CheckAuthorization(AllowUserInteraction)
        ▼
polkitd (org.freedesktop.PolicyKit1)
        │ 找到当前 session 注册的 agent
        │ BeginAuthentication(action, message, icon, details, cookie, identities)
        ▼
Clarion polkit agent（系统总线对象）
        │ 显示密码对话框；启动/连接 polkit-agent-helper-1
        ▼
polkit-agent-helper-1（setuid 或 socket activated）
        │ PAM（polkit-1 service）
        │ 认证成功后由 helper 自己调用
        │ AuthenticationAgentResponse2(uid, cookie, identity)
        ▼
polkitd 放行原调用者
```

关键点：

1. **Clarion 不直接调用 `AuthenticationAgentResponse2`**。这由
   `polkit-agent-helper-1` 完成，agent 只负责注册、展示 UI、把 PAM 问答转给用户。
2. `BeginAuthentication` 的 D-Bus 方法应保持 pending，直到认证成功、失败或取消；
   成功后返回 `Ok(())`，用户取消返回
   `org.freedesktop.PolicyKit1.Error.Cancelled`。
3. cookie 只能通过 stdin / Unix socket 传给 helper，不能放 argv（CVE-2015-4625）。

### helper 通信协议

优先使用新 polkit 的 socket：

```
/run/polkit/agent-helper.socket
```

连接后依次写入：

```
<username>\n
<cookie>\n
```

随后在同一连接上读写 PAM 会话。老系统回退为：

```
/usr/lib/polkit-1/polkit-agent-helper-1 <username>
```

cookie 写入子进程 stdin。helper 输出逐行：

| 行前缀 | 含义 |
| --- | --- |
| `PAM_PROMPT_ECHO_OFF <text>` | 需要用户输入（不回显，通常为密码） |
| `PAM_PROMPT_ECHO_ON <text>` | 需要用户输入（回显） |
| `PAM_TEXT_INFO <text>` | 提示信息 |
| `PAM_ERROR_MSG <text>` | 错误信息 |
| `SUCCESS` | 认证成功，helper 已完成 D-Bus 响应 |
| `FAILURE` | 认证失败 |

文本经过 GLib `g_strescape` 转义，需要实现 `g_strcompress` 解码
（`\\ \" \n \r \t \b \f \v` 和八进制转义）。用户回答直接写原文 + `\n`。

## 二、Shell 内架构

```
clarion-shell/src/
├── polkit/
│   ├── mod.rs          # 纯逻辑：identity、subject、cookie 队列、PAM 行解析
│   ├── helper.rs       # helper socket / setuid 子进程会话
│   └── tests.rs
├── services/
│   ├── polkit.rs       # 系统总线连接、agent 注册、zbus 对象
│   └── ...             # 现有通知/托盘服务
├── view/polkit.rs      # 认证对话框渲染
├── input.rs            # TextInput secure/mask 模式
├── app.rs              # Host 状态机与窗口生命周期
└── view.rs             # Surface::Auth 的输入区域、键盘、磨砂区域
```

### 线程与事件

与现有 `services` 模式一致：**UI 线程绝不做 D-Bus / PAM I/O**。

- 新增专用线程 `clarion-polkit`，`async_io::block_on` 运行系统总线事件循环。
- `services::Event` 增加：
  - `AuthRequest(AuthRequest)`（首个请求或队列推进）
  - `AuthPrompt { cookie, text, echo }`
  - `AuthInfo { cookie, text }`
  - `AuthError { cookie, text }`
  - `AuthClosed { cookie }`
  - `AuthStatus { registered, reason }`（注册失败/冲突/缺少 helper）
- `services::Command` 增加：
  - `AuthSubmit { cookie, secret }`
  - `AuthCancel { cookie }`
  - `PolkitSetEnabled(bool)`（偏好开关，P2）
- `ServiceHandle` router 增加第三个目标 `polkit`，或把 polkit 命令直接放进独立
  `ServiceHandle`。

### Host 状态机

`Host` 新增独立认证窗口，而不是复用 `Panel`：

```rust
struct AuthDialog {
    request: AuthRequest,
    prompt: String,
    echo: bool,
    error: Option<String>,
    info: Option<String>,
    submitting: bool,
}

auth: Option<AuthDialog>,          // 当前显示
auth_window: Option<WindowHandle<ShellView>>,
auth_queue: VecDeque<AuthRequest>, // 或由 service 持有队列
```

使用独立窗口的原因：

- 不调用 `Host::open()`，不会 `discard()` 掉用户正在用的 Spotlight / 设置面板；
- 认证窗口可以始终在 Overlay 层并拿到 Exclusive 键盘焦点；
- 关闭后能把焦点还给原窗口。

窗口形态与 Spotlight action card 相同：**全屏透明 Overlay 窗口 + 居中卡片**。
卡片外的区域绘制 scrim 并拦截点击（模态）；输入区域只覆盖卡片 + scrim。

### 窗口与焦点

- 新 `Surface::Auth`，`LayerShellKeyboardInteractivity::Exclusive`。
- 出现在 `active_output()`；polkit 请求不携带输出信息，取当前鼠标/焦点所在屏。
- `Esc` = 取消；`Enter` = 提交；输入框自动聚焦。
- shell 退出、禁用代理、polkitd 消失时：关闭 helper、取消所有 pending、
  注销 agent、移除窗口。
- 认证完成/取消后，恢复之前的 `popup` 或 `spotlight_menu` 焦点。

### 注册与重注册

系统总线：

- name：`org.freedesktop.PolicyKit1`
- path：`/org/freedesktop/PolicyKit1/Authority`
- interface：`org.freedesktop.PolicyKit1.Authority`

注册步骤：

1. 确定 session：
   - 优先 `XDG_SESSION_ID`；
   - 回退 logind `Manager.GetSessionByPID(our_pid)`，再读 `Session.Id`；
   - 无法确定时不注册，上报 `AuthStatus`。
2. 调用 `RegisterAuthenticationAgentWithOptions(subject, locale, agent_path, options)`：
   - `subject = ("unix-session", {"session-id": ...})`
   - `options = {"fallback": false}`；老 polkit 不支持 WithOptions 时回退
     `RegisterAuthenticationAgent`。
3. agent 对象路径：`/org/clarion/PolicyKit1/AuthenticationAgent`
   （在本进程 unique name 上，不需要 request_name）。
4. 监听 `org.freedesktop.PolicyKit1` 的 `NameOwnerChanged`：
   - polkitd 消失：标记未注册，取消当前请求；
   - polkitd 回来：重新注册。
5. 已有其他 agent 时注册会失败（`already exists`）：
   - 每秒/退避重试；
   - 生成一次性提示：“已有其他 polkit 认证代理在运行，Clarion 无法接管”。
   - 真正的接管要求用户停用 polkit-gnome / GNOME 自带 agent；
     `fallback: true` 只能作为“其他 agent 不在时兜底”，不能抢占。

`begin_authentication` 处理：

- 用 `#[zbus(header)]` 取 sender，调用
  用 `#[zbus(header)]` 取 sender，与 `GetNameOwner(org.freedesktop.PolicyKit1)`
  的当前 owner 比较（polkitd 现在以 `polkitd` 用户运行，不是 uid 0；
  D-Bus policy 已限制，这里做纵深防御）。
- 解码 `identities: Vec<(String, HashMap<String, OwnedValue>)>`；P1 只支持
  `unix-user`，取 uid 并解析用户名。
- 校验 cookie 长度/字符；每个 cookie 只允许一个 pending 请求。
- 放进队列；若当前没有对话框，立即发 `AuthRequest`。
- 在 async 方法内等待 `oneshot`/`async_channel` 的完成结果：
  - `Ok(())` → 方法返回成功；
  - 用户取消 → 返回自定义 `PolicyKitError::Cancelled`；
  - helper 失败 / 无 identity → `Error.Failed`。
- `CancelAuthentication(cookie)`：关闭 helper、标记取消、唤醒 pending 方法，
  返回 `Error.Cancelled`。

自定义错误名：

```rust
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.PolicyKit1.Error")]
enum PolicyKitError {
    Cancelled,
    Failed(String),
}
```

## 三、界面设计（对齐参考图）

参考图是 macOS 原生权限/警告 dialog：白色圆角卡片、顶部居中图标、加粗标题、
说明正文、蓝色主按钮和灰色次按钮。认证弹窗按这个方向做，但使用 Clarion 的
字体与调色板。

### 形态

- 全屏 scrim 使用 `p.scrim`，卡片居中；
- 卡片宽 **340**（窄屏 `min(340, 可用宽 - 48)`），高度由
  `view::polkit::auth_card_size()` 精确计算，绘制与合成器磨砂区域共用同一尺寸；
  默认卡片高 > 宽（纵向权限提示，区别于启动台的横向大面板）；
- 圆角 **32**，1px `p.line` 边框，背景 `p.panel`（默认 68% 不透明度 + frost wash），
  半透明磨砂而不是不透明白卡：
  - `Surface::Auth` 的 glass region 严格等于卡片圆角矩形，卡片外保持清晰；
  - 关闭磨砂偏好时只保留半透明底色；
- 不加外侧投影（GPUI 的 box shadow 不随大圆角裁剪）。

### 内容层级

```
                 ┌──────────────────────────┐
                 │                      [?] │
                 │          [icon 60]       │
                 │        bold title        │
                 │   message / action text  │
                 │   details（可选，11px）  │
                 │  [ password input 34px ] │
                 │  error / info（11px）    │
                 │        [取消] [认证]     │
                 └──────────────────────────┘
```

- 顶部居中图标（88×88，圆角 23，图形 60）：
  - 优先使用请求进程的桌面图标：从 `polkit.subject-pid`（pkexec 时就是父 shell）
    或 `polkit.caller-pid` 读 `/proc/<pid>/exe`，匹配 `.desktop` 的 `Exec`/`Icon`；
    遇到 `pkexec`/`sudo`/`systemctl` 等启动器会向上走到真正的终端/应用；
  - 回退 polkit `icon_name`，再回退 `dialog-password-symbolic`。
- 标题：polkit `message`，17px semibold，居中；按宽度自动换行（最多三行，
  行高 23），不再单行居中裁切。
- 副标题：`<应用> 请求以 <用户> 的身份认证`，13.5px muted，单行省略。
- 详情：过滤 `polkit.*` 内部键（如 `polkit.caller-pid` / `polkit.subject-pid`），
  最多两条（12px，行高 16），放在浅色 inset 圆角块；没有可读内容时不占空间。
- 密码框：
  - 高度 48，胶囊形（圆角 = 高度/2），聚焦时 accent 描边，17px 字；
  - 不再显示独立 prompt label；框内左侧显示 18px 锁图标
    （`system-lock-screen-symbolic`，echo 提示时显示人像），placeholder 为“密码”；
  - 项目符号遮罩必须真正进入绘制：`SpikeTextElement` 用等长 `●` 文本 shaping，
    输入框容器需要 `flex_1 + min_w_0`，否则宽度塌陷会让密码看起来是空的；
  - 错误后清空并保持焦点；
  - 禁止复制/剪切，允许粘贴；
  - IME 预编辑文本也遮罩，不进入无障碍树。
- 错误行：12.5px，`#FF453A`；PAM 重试时保留对话框。
- 按钮（macOS 规则）：
  - 高 44，胶囊形（圆角 = 高度/2），15px 字；单个按钮铺满整行，
    多个按钮在一行内 `flex_1` 平分；
  - 两个按钮：取消在左（`p.card`），认证在右（`p.accent` 蓝底白字）；
  - 提交中主按钮降为 55% accent 并禁用；
  - 键盘：`Enter` 主按钮，`Esc` 取消。
- 动效：120–140ms 淡入 + 0.96→1 缩放；`reduce_motion` 时直接显示。
- 多个请求：**一次只显示一个**，其余排队；可在副标题轻量显示
  “还有 N 个认证请求”。不采用参考图的多窗口叠放，避免同时输入多个密码。

## 四、secure TextInput

现有 `input.rs` 的 `TextInput` 需要增加：

```rust
secure: bool,
```

实现原则：

- 真实内容仍保存在 `content`，输入/删除/选择/IME 全部照常；
- 绘制时 shaping 一个等长的 `••••` 字符串，光标和选区按真实内容计算；
- `Ctrl+C` / `Ctrl+X` 在 secure 模式下禁用；
- `Ctrl+V` 允许；
- accessibility 不暴露真实文本；
- 重置对话框时用 `zeroize`（新增小依赖）或显式覆写并清空。

## 五、安全约束

- 只接受 `org.freedesktop.PolicyKit1` 当前 owner 发来的
  `BeginAuthentication` / `CancelAuthentication`（不能检查 uid 0：polkitd 以
  `polkitd` 用户运行）。
- cookie 只走 stdin / socket，不走 argv、环境变量、日志。
- 密码不进入 `Debug`、日志、通知、剪贴板、历史；错误信息只显示 PAM 文本。
- helper 启动时清空环境，只保留 `PATH`、`LANG`、`LANGUAGE`、`LC_MESSAGES`。
- 关闭认证窗口必须取消 helper，不能留下悬挂进程/连接。
- shell 退出时注销 agent；崩溃时 helper 随连接关闭自行退出。
- 不自己实现 PAM，不写 setuid 程序；只使用发行版提供的
  `polkit-agent-helper-1`。
- 认证代理注册的是 shell 所在 session，不使用 `fallback` 冒充接管。

## 六、兼容与部署

- polkit ≥ 0.114：helper 使用 `AuthenticationAgentResponse2`（对 agent 透明）。
- polkit 新版（当前机器为 128）：优先 `/run/polkit/agent-helper.socket`，
  无需 setuid。
- 老版本：探测
  `/usr/lib/polkit-1/polkit-agent-helper-1`、
  `/usr/libexec/polkit-1/polkit-agent-helper-1`、
  `/usr/lib64/polkit-1/polkit-agent-helper-1`。
- `RegisterAuthenticationAgentWithOptions` 不存在时回退旧方法。
- 安装脚本只做检测与提示，不修改系统 polkit 配置。
- 文档说明：要真正接管，需要停用/移除 `polkit-gnome-authentication-agent-1`
  或桌面环境自带的认证代理。
- 偏好项（P2）：`polkit_agent` 默认开启；关闭时注销并关闭当前弹窗。
  状态区可显示：已接管 / 被其他 agent 占用 / 缺少 helper / 无 session。

## 七、分期

### P1：协议与最小可用

1. `polkit/` 纯逻辑：identity、subject、cookie 队列、PAM 行解析与转义解码。
2. `polkit/helper.rs`：socket 优先、setuid 回退、PAM 问答、取消。
3. `services/polkit.rs`：系统总线注册、zbus agent 对象、polkitd 重连。
4. `Host` + `Surface::Auth` + 最小对话框：密码、认证、取消、错误。
5. 测试：假 helper 脚本、纯逻辑单测、UI 基础测试。

验收：`pkexec /bin/true` 能弹出 Clarion 对话框并完成认证；取消返回
`Error.Cancelled`；密码错误可重试。

### P2：macOS 风格与完整交互

1. 参考图样式：图标、标题、正文、inset 详情、蓝/灰按钮、scrim、动效。
2. secure TextInput、粘贴/禁用复制、IME 遮罩。
3. 多请求队列、队列计数、失败/信息提示。
4. `?` 详情展开、identity 选择（多个 unix-user）。
5. 偏好开关、状态提示、与已有 agent 冲突的诊断。

### P3：部署与硬化

1. 安装脚本检测 helper / 其他 agent / session。
2. 文档：如何停用 polkit-gnome、systemd user service 下的 session 检测。
3. 安全审查：uid 校验、cookie 路径、密码内存、日志扫描。
4. 真实场景回归：polkitd 重启、shell 重启、并发请求、锁屏。

## 八、测试与验收

### 单元测试

- `g_strescape` 解码：引号、反斜杠、换行、八进制、UTF-8。
- PAM 行状态机：prompt/info/error/SUCCESS/FAILURE 顺序。
- cookie 队列：重复 cookie、并发、取消当前后推进下一个。
- identity 解析：unix-user uid、缺失用户名、非 unix-user。
- session 解析：`XDG_SESSION_ID`、logind 回退、无 session。

### 集成测试

- 假 helper 可执行文件（测试注入路径）：
  - 正常密码 → SUCCESS；
  - 错误密码 → PAM_ERROR_MSG + 再次 prompt；
  - 取消 → 进程/连接关闭。
- 私有 D-Bus 夹具 + mock authority：调用 `BeginAuthentication`，
  验证 pending、成功回复、取消错误名、uid 校验。
- 测试环境绝不能触发真实 PAM 或真实系统授权。

### UI 测试

- 请求到来时打开 `Surface::Auth`，图标/标题/正文/用户名正确。
- 输入遮罩、Enter 提交、Esc 取消、错误后保持焦点并清空。
- 提交中 spinner、禁用按钮。
- 认证完成后窗口关闭、焦点回到原面板。
- 队列：第二个请求在当前关闭后显示。
- 不支持/缺 helper/注册冲突的状态提示。

### 手工验收

```sh
# 成功
pkexec /bin/true

# 错误密码后重试、取消
pkexec /bin/false

# 并发请求
pkexec /bin/true & pkexec /bin/true &

# polkitd 重启后仍能工作
sudo systemctl restart polkit

# shell 重启后重新注册
systemctl --user restart clarion-shell
```

确认：

- 对话框只出现在当前活动输出；
- 没有 polkit-gnome 也能工作；
- 密码不出现在 `ps`、日志、剪贴板；
- 取消后调用方收到取消而不是永久挂起。

## 九、待定问题

1. 锁屏/会话锁定时是否允许弹认证框；建议 P3 再定，P1 先保持隐藏。
2. 是否支持 `unix-group` identity 与多身份切换；P2 视需求。
3. 是否允许从密码管理器粘贴；默认允许粘贴，禁止复制。
4. 是否在认证弹窗上额外显示请求方 PID/应用名（`details` 可提供）；
   参考图显示 “Fantastical would like to access…”，可在 P2 用 action message
   代替，不伪造应用身份。
5. 现有 GPUI 对多窗口焦点抢占的行为需在真实 compositor 下验证；
   P1 先用 LayerShell Exclusive 全屏 overlay。

## 十、实现状态（2026-11-08）

P1（协议 + 最小可用）与 P2 的视觉/secure input/队列计数部分已落地，尚未提交：

- `clarion-shell/src/polkit/mod.rs`：`AuthRequest`/`AuthIdentity`/`AuthStatus`、
  cookie 校验、`g_strescape` 解码、PAM 行解析、uid→用户名、identity 选择、
  unix-session subject。
- `clarion-shell/src/polkit/helper.rs`：socket 优先（`/run/polkit/agent-helper.socket`），
  setuid helper 回退；用户名/cookie 只走 stdin/socket；PAM 问答；
  socket shutdown / child kill+wait 取消。
- `clarion-shell/src/services/polkit.rs`：系统总线连接、`Agent` zbus 对象、
  authority owner 校验、pending `BeginAuthentication`、取消、同 cookie 重试、
  队列、注册/重注册、注册失败退避（10s）、logind session 回退
  （跳过 `manager` 伪 session，优先 wayland/x11 且有 seat 的活跃 session）。
- `clarion-shell/src/view/polkit.rs`：420px、圆角 18、近不透明卡片、scrim、
  居中图标/标题/身份/详情、secure 输入框、灰取消 + 蓝认证、错误/信息/队列计数。
- `clarion-shell/src/input.rs`：`TextInput::with_secure()`，逐字符 `•` 遮罩，
  caret/selection/IME 偏移映射，禁用复制/剪切，允许粘贴。
- `clarion-shell/src/app.rs` + `view.rs`：`Surface::Auth` 独立 Overlay、
  `AuthSubmit`/`AuthCancel`、Host 快照更新、`Esc`/`Enter`、
  窗口被平台关闭时自动取消；面板的 deactivation 逻辑把认证窗口视为
  “菜单类覆盖层”，不会丢弃原面板。
- 测试：polkit 纯逻辑 8 条、helper 5 条（含真实 socket/子进程传输）、
  secure 遮罩 1 条、auth UI 12 条，共新增 26 条通过用例；
  auth UI 覆盖：卡片/磨砂尺寸、按钮等宽、长文本换行、技术 details 过滤、
  echo 用户名、错误重试、info/error/队列增高、提交中忽略二次 Enter、
  窗口关闭取消、Esc 取消/完成关闭等。
  `clarion-shell` 全量 375 passed / 2 ignored（基线 349 + 26；其中 1 条
  `#[ignore]` 是需手动启用的真实系统总线注册探针，已用
  `CLARION_POLKIT_LIVE_TEST=1` 在本机验证 `AuthStatus { registered: true }`）。
- 本机部署：`polkit-gnome-authentication-agent-1` 是合成器按
  `~/.config/clarion/config.kdl` 的 `spawn-sh-at-startup` 拉起的，
  `XDG_SESSION_ID=3`，会一直占用 session 3 的代理。已于 2026-11-08
  结束该进程并注释掉那一行；`CLARION_POLKIT_LIVE_TEST=1` 下的真实系统总线
  探针已返回 `AuthStatus { registered: true }`。
- 已安装 release：`315bf2b2d3d87aadfb3c947c44d7865d8ff2192f66f7a2b85f3ab8f595a539ba`。
  下一步由用户重启 `clarion-shell` 后用 `pkexec` 验收。

### 本机验收步骤

```sh
# 1. 停用 polkit-gnome 后重启 shell
systemctl --user restart clarion-shell
journalctl --user -u clarion-shell -n 50 | grep polkit

# 2. 应弹出 Clarion 原生认证卡片
pkexec /bin/true

# 3. 取消应让调用方立刻失败，而不是挂起
pkexec /bin/true   # 点“取消”

# 4. 错误密码可重试
pkexec /bin/false
```

注意：`AuthClosed` 目前只携带 `cookie`，不再有 outcome；助手失败后的
`AuthFailed` 会保留对话框供重试。P2 剩余项（`?` 详情展开、多 identity 选择、
偏好开关/状态区、缩放/淡入动效、reduce-motion）与 P3（安装脚本检测、
`systemd --user` 下的 session 检测回归、安全审查）尚未实现。
