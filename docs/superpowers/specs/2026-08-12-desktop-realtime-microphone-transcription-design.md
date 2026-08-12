# 桌面端实时麦克风转写设计

## 背景与目标

在聊天输入框发送按钮旁增加实时麦克风转写入口。用户点击后开始录音，识别结果实时显示在输入框末尾；再次点击停止后，最终文本保留为普通草稿，用户可以继续编辑或手动发送。

本功能只在 Tauri 桌面窗口中提供。纯 Web 和 `iyw-claw-server` 页面不显示入口、不申请麦克风权限。桌面窗口即使连接了远程工作区，音频采集、账户认证和 Fusion WebSocket 仍在本机完成，不经远程工作区 transport。

## 范围

### 包含

- WebView 麦克风采集与 `16kHz / 16bit / mono PCM` 转换。
- Rust 侧读取当前 IYW 账户 token，连接 Fusion 实时语音接口。
- `ready`、`partial`、`final`、`completed` 和 `error` 事件处理。
- 输入框末尾的临时识别预览与最终文本追加。
- 麦克风按钮、录音状态、停止操作和错误反馈。
- “停止后自动发送”本机设置，默认关闭。
- 正常停止、取消、异常、组件卸载和窗口退出时的资源释放。

### 不包含

- Web/Server 模式录音。
- 录音文件持久化、历史录音列表或回放。
- 音频设备选择、语言选择或语音参数设置页面。
- 修改 Fusion 实时协议、账户登录流程或文件录音转写工具。
- 数据库迁移、Apollo 配置或新的 Cargo/npm 依赖。

## 方案比较

### 方案 A：WebView 采集，Rust 连接 Fusion（采用）

WebView 使用浏览器音频 API 采集麦克风并生成 PCM；Rust 复用现有 IYW 账户 token 和 `tokio-tungstenite` 连接 Fusion。前端只向本机 Tauri IPC 发送音频块，不接触 token。

优点是跨平台采集成本可控、认证边界清晰，并可复用现有依赖和 Fusion 地址配置。缺点是需要维护一条 WebView 到 Rust 的有界音频流。

### 方案 B：WebView 直接连接 Fusion

实现量较少，但必须将当前登录 token 暴露给 WebView JavaScript，不符合桌面端凭据边界，因此不采用。

### 方案 C：Rust 原生采集并连接 Fusion

token 和音频均不进入 WebView，但需要引入跨平台音频设备、权限和重采样依赖，平台差异和维护成本较高，当前需求不采用。

## 架构与职责

### 前端音频模块

独立音频采集模块负责：

1. 通过 `navigator.mediaDevices.getUserMedia` 请求单声道麦克风。
2. 创建 `AudioContext` 和 `AudioWorkletNode`。
3. 根据实际输入采样率连续重采样为 16kHz，转换成有符号 16 位单声道 PCM。
4. 以 100 至 200 毫秒为一个音频块交给实时语音 hook。
5. 停止所有 `MediaStreamTrack`，断开节点并关闭 `AudioContext`。

Worklet 作为静态资源随 Next.js 静态导出产物发布。重采样器必须跨输入回调保留余数和相位，不能逐块独立重采样，否则会在块边界丢样或重复采样。

### Rust 实时语音模块

仅在 `tauri-runtime` 下注册本机 Tauri 命令，负责：

- 从现有账户会话读取 `access_token`。
- 复用 `IYW_CLAW_FUSION_API_BASE_URL` 和环境对应的默认 Fusion 地址，将 HTTP(S) 基址转换为 WS(S) 地址并拼接 `/v1/voice/realtime/connect`。
- 建立 WebSocket 后立即发送首个 `auth` 文本帧，不向前端返回或记录 token。
- 将前端传入的 PCM 音频块作为二进制帧发送。
- 正常停止时发送 `{"type":"finish"}` 并等待 `completed` 或 `error`。
- 通过本次启动命令携带的 Tauri channel 将协议事件返回给调用窗口。
- 使用有界队列、连接/认证超时和停止等待超时，避免慢连接导致无界内存增长或悬挂任务。

每个窗口最多有一个活动录音会话。新会话不得静默覆盖旧会话；重复开始返回明确错误。音频块、停止和取消命令都携带不可猜测的本机会话 ID，Rust 拒绝过期或不匹配的调用。

### React 实时语音 hook

独立 hook 维护以下状态机：

```text
idle -> starting -> recording -> stopping -> idle
                  \-> error -> idle
starting/recording/stopping -> cancelling -> idle
```

hook 负责串联麦克风、Tauri 命令和事件，不将网络与音频生命周期堆入现有体积较大的 `message-input.tsx`。组件卸载、会话切换或窗口失焦不自动停止；只有组件卸载、聊天上下文销毁、用户停止、错误或取消才结束会话。页面隐藏不停止，以免系统弹窗或短暂切换窗口造成录音中断。

## Fusion 协议

WebSocket 建立后 5 秒内发送：

```json
{
  "type": "auth",
  "token": "<current account access token>",
  "audio": {
    "format": "pcm_s16le",
    "sampleRate": 16000,
    "bitsPerSample": 16,
    "channels": 1
  },
  "language": "zh-CN",
  "options": {
    "punctuation": true,
    "interimResults": true,
    "wordTimestamps": false
  }
}
```

收到 `ready` 后才能接收音频块。前端在 Rust 返回 ready 之前不启动音频投递，避免音频先于认证进入连接。

正常停止发送：

```json
{"type":"finish"}
```

Rust 只接受已知事件类型和合法字段；未知事件记录脱敏诊断后忽略。事件必须附加本机会话 ID，前端据此忽略旧会话的迟到消息。

## 草稿与识别文本合并

### Partial

`partial` 是临时结果，不写入 ProseMirror 文档，也不写入草稿持久化。`RichComposer` 在文档末尾用 decoration/widget 显示当前 partial，并以弱化样式区分未确认内容。

这样用户在录音期间可以任意移动光标、输入、删除或修改已经确认的文本。文档变化后 decoration 自动跟随新的文档末尾，后续 partial 只替换当前临时预览，不回滚或覆盖用户编辑。

### Final

收到新的 `final.sequence` 后：

1. 清除当前 partial 预览。
2. 将非空 final 文本插入当时 ProseMirror 文档的真实末尾，而不是当前光标位置。
3. 文本进入普通草稿，之后完全由用户编辑。
4. 记录已处理的 sequence，重复或倒序事件不再次追加。

如果既有草稿末尾和 final 开头都是 ASCII 字母或数字，则补一个空格；中文、标点和已有空白直接连接，避免无条件插入空格破坏中文句子。

用户删除先前 final 后，后续 final 仍追加到用户编辑后的当前末尾。识别层不保存一份权威草稿副本，也不尝试恢复被用户删除的文字。

### Completed

`completed` 表示服务端已经完成正常收尾。此时清除遗留 partial，释放会话资源，并根据自动发送条件决定是否调用输入框现有发送逻辑。

## 交互设计

- 麦克风按钮位于发送按钮左侧，使用 Lucide `Mic` 图标和现有 `icon-xs` 按钮尺寸。
- 只要处于 Tauri 桌面窗口就显示，包括连接远程工作区的桌面窗口；纯浏览器不渲染该按钮。
- `idle` 时点击开始；`starting` 时显示不可重复触发的等待状态；`recording` 时按钮使用明确的录音中状态，再次点击进入停止；`stopping` 时禁用重复点击。
- 按钮 title/aria-label 根据状态显示“开始语音输入”“停止语音输入”等本地化文案。
- 麦克风按钮自己的右键菜单包含“停止后自动发送”复选项。设置保存在 `localStorage`，无效或缺失值一律回退为关闭。
- 默认停止后不发送，final 文本留在输入框供用户检查和编辑。
- 录音过程中发送按钮保持现有规则，但不允许提交正在显示的 partial。若用户主动发送已确认草稿，必须先正常停止并等待 completed；UI 不提供绕过该顺序的路径。

## 自动发送条件

自动发送只有同时满足以下条件才执行一次：

- 设置已开启。
- 本次结束由用户点击“停止”触发，而不是错误、取消、卸载或上下文销毁。
- 已收到 `completed`。
- 本次录音至少追加过一个非空 final。
- 当前没有 partial，输入框存在可发送内容。
- 输入框当前允许走既有 `handleSend` 语义，且不处于队列编辑、发送中或其他禁止发送状态。

任一条件不满足时只保留草稿，不发送。自动发送复用现有 `buildDraft` 和 `handleSend`，不另建消息提交路径，因此附件、模式选择、草稿清理和错误处理保持一致。

## 错误与资源释放

- 未登录：不申请麦克风或立即释放已申请资源，提示用户先登录。
- 麦克风 API 不可用、权限拒绝、没有设备：显示对应本地化错误，草稿不变。
- Fusion 连接、认证、上游忙或协议错误：停止采集，清除 partial，保留所有已提交 final，不自动发送。
- 音频队列满或 IPC 失败：视为会话错误并取消，避免继续录音但实际丢失音频。
- 用户取消、组件卸载或上下文销毁：关闭采集和 WebSocket，不发送 finish，不自动发送。
- 用户正常停止：先停止采集，保证最后一个 PCM 块入队，再发送 finish，等待 completed；超时则按错误结束。
- 所有结束路径都必须幂等释放 MediaStream、AudioContext、队列、WebSocket 任务和 channel。

日志只记录脱敏的会话状态、事件类型、sequence、音频字节数、耗时和错误码。禁止记录 token、PCM 内容、转写正文或完整上游响应。

## 平台权限

Windows/Linux 使用 WebView 的媒体权限请求。macOS 新增 `src-tauri/Info.plist`，写入 `NSMicrophoneUsageDescription`，由 Tauri 在打包时合并进应用清单；文案说明麦克风仅用于将语音转换为输入文字。不扩大其他 Tauri capability。

## 预计文件边界

- `src-tauri/src/commands/realtime_voice/mod.rs`：Tauri 命令和公开类型。
- `src-tauri/src/commands/realtime_voice/session.rs`：单窗口会话注册、有界音频队列和资源清理。
- `src-tauri/src/commands/realtime_voice/client.rs`：Fusion 地址、认证首帧、WebSocket 收发和协议校验。
- `src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`：模块与命令注册。
- `src/lib/realtime-voice.ts`：桌面命令、channel 类型和自动发送设置封装。
- `src/hooks/use-realtime-voice-input.ts`：前端状态机与资源生命周期。
- `src/components/chat/composer/rich-composer.tsx`：末尾 partial decoration 和末尾追加接口。
- `src/components/chat/message-input.tsx`：按钮、右键设置与现有发送逻辑接线。
- `public/realtime-pcm-worklet.js`：连续重采样和 PCM 分块。
- `src/i18n/messages/en.json`、`src/i18n/messages/zh-CN.json`：用户可见文案。
- `src-tauri/Info.plist`：macOS 麦克风用途声明。

## 验证方案

遵循仓库本机执行限制，不运行测试、`pnpm build`、Tauri build 或任何 Cargo build/check/test/clippy，也不新增或修改测试文件。

交付前执行：

1. 静态审查完整调用链：按钮 -> 麦克风 -> PCM -> Tauri IPC -> Fusion -> channel -> partial/final -> 草稿 -> 可选发送。
2. 审查正常停止、权限拒绝、认证失败、连接失败、上游 error、取消、组件卸载和迟到事件路径。
3. 检查 token、音频和转写正文不会进入日志或前端状态持久化。
4. 检查 Web/Server 模式不会渲染入口或请求权限。
5. 对本任务文件执行格式检查和 `git diff --check`。
6. 精确检查暂存内容，确保不夹带脏工作树中的并发改动。

运行时麦克风权限、实际音频质量、WebSocket 兼容性和打包后的 macOS 用途声明只能由远端 CI/桌面环境验证，交付时必须明确说明该验证边界。

## 验收标准

- Tauri 桌面聊天输入框发送按钮旁显示麦克风；Web/Server 模式不显示。
- 点击开始后可看到实时 partial，用户编辑不被 partial 覆盖。
- final 始终追加到用户当前草稿末尾并可自由编辑。
- 再次点击正常停止，默认不发送且草稿保留。
- 自动发送默认关闭，可通过右键菜单持久化切换。
- 自动发送只在正常 completed 且存在本次非空 final 时触发。
- 错误、取消、空识别和卸载均不自动发送，不丢失原草稿或已确认 final。
- token 仅由 Rust 从现有账户会话读取，不传给前端、不写日志。
- 不保存录音，不新增数据库、Apollo 配置或第三方依赖。
