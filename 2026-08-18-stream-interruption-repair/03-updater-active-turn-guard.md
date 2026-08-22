# 修复方案 03：更新安装活动回合门禁

## 目标

允许更新包在后台下载和校验，但 Windows NSIS 安装器启动前必须再次检查活动回合。默认等待回合自然结束；只有用户明确确认时，才能取消活动回合并继续安装。

## 已确认现象

`iyw-claw.2026-08-18.log` 的顺序为：

1. 00:55:30，连接 `ec53f8a0-dffa-4de8-abcf-47f1ee332ebf` 记录 `prompt started`。
2. 日志中没有该连接对应的 `prompt completed`、`prompt failed` 或 `prompt interrupted`。
3. 01:01:27，记录 `[app-update] launching NSIS updater; the desktop process will exit`。
4. 01:01:41，新版本 `0.1.82` 进程启动。

这能确认更新导致桌面进程退出；日志不能证明 Agent 当时已经完成，因此安装边界必须以运行状态二次检查为准。

## 当前代码缺口

- `src-tauri/src/update/release.rs::install_update` 下载完成后立即调用 `update.install(bytes)`。
- `src-tauri/src/commands/app_update.rs::perform_app_update` 没有注入 `ConnectionManager`，也没有活动回合门禁。
- `desktop_updater().on_before_exit` 会执行 shutdown cleanup，但它只是在进程即将退出时清理资源，不能保护未完成的业务回合。
- `ConnectionManager::list_active_sessions` 面向 UI，不能直接作为安装权威判断：它还包含 Error，会漏掉部分内部 turn/background 状态。

## 具体改动

### 1. 增加内部安装阻塞快照

在 `ConnectionManager` 新增只供后端使用的方法：

```rust
async fn app_update_blockers(&self) -> AppUpdateBlockerSnapshot
```

阻塞条件包括：

- `status == Prompting`。
- `turn_in_flight` 或 `turn_completion_pending`。
- pending permission、question、channel confirmation。
- native background turn、活动 tool call、delegation 或 terminal。
- 会影响回合结果提交的后台任务。

快照只返回计数以及 `connection_id/conversation_id/turn_generation`，不返回 prompt 内容。

### 2. 拆分 Downloaded 与 Installing

修改：

- `src-tauri/src/update/release.rs`
- `src-tauri/src/update/state.rs`
- `src-tauri/src/commands/app_update.rs`
- `src/lib/updater.ts`
- `src/components/providers/update-provider.tsx`
- `src/components/layout/title-bar-update-control.tsx`

Windows 流程调整为：

```text
Available
  -> Downloading
  -> Verifying
  -> Downloaded
  -> WaitingForIdle（存在 blocker）
  -> Installing
```

下载完成后把已校验字节写入应用私有 staging 文件，例如：

```text
<agent-storage>/runtime/downloads/app-update/<version>/<sha256>.nsis
```

使用临时文件写入、`sync_all`、原子 rename；文件名和目录都由服务端已验证的版本及 SHA-256 生成，不接受前端路径。进入 Installing 前重新检查大小、SHA-256 和 update identity，然后读入 `update.install(bytes)`。

更新完成、取消、校验失败或版本被替换时，只删除这一条明确记录的 staging 文件。不得递归清理整个 downloads 目录。

### 3. 在 install 前做权威二次检查

下载开始前可以检查一次，用于提前提示；真正门禁必须位于 `update.install(bytes)` 的前一条业务路径：

1. 调用 `app_update_blockers()`。
2. 无 blocker：CAS 将 update state 从 Downloaded 切到 Installing，然后调用 NSIS。
3. 有 blocker：切到 WaitingForIdle，保存 blocker count，不调用 NSIS。
4. 后台等待只订阅 Agent 状态变化或使用 1 秒低频检查；连续 3 秒无 blocker 后再次校验包并进入 Installing。
5. WaitingForIdle 期间如果出现新 blocker，继续等待，不取消任何回合。

CAS 必须同时校验 version、release identity 和 staging package identity，防止旧窗口安装过期包。

### 4. 明确用户强制安装语义

新增命令参数或独立命令：

```text
confirm_app_update_install(cancel_active_turns: true)
```

只有用户在明确确认对话框中选择“取消进行中的任务并安装”后才能调用。后端流程：

1. 重新获取 blocker snapshot。
2. 对每个 blocker 使用 `connection_id + turn_generation` 发送 cancel，避免取消已经开始的新回合。
3. 最多等待 10 秒让所有回合进入 terminal。
4. 仍未结束时再次向用户返回 blocker，不默认强制杀进程。
5. 用户第二次明确确认强制退出后，才调用现有 shutdown cleanup 并安装。

普通“安装更新”按钮不能携带 `cancel_active_turns=true`。

### 5. 保留最终退出清理

`on_before_exit` 和 `desktop_shutdown` 保留为最终资源清理：

- 停止新 MCP/Agent 请求。
- 取消并回收连接、Runtime Host 和终端。
- 写完必要状态后退出。

它不能替代 install gate，也不能把未完成回合标记为成功。

## 结构化日志

只在状态转换时记录：

```text
version, release_id, update_state,
blocker_count, prompting_count, pending_interaction_count,
staged_bytes, staged_sha256_prefix, wait_ms, decision
```

不在 1 秒等待检查中重复打印。等待开始、blocker 数发生变化、进入安装或用户取消时各记录一次。

## 聚焦验证

1. 无活动回合：下载、校验、安装流程保持现状。
2. 下载期间启动新 prompt：下载完成后进入 WaitingForIdle，桌面进程不退出。
3. 回合自然完成：稳定空闲 3 秒后只安装一次。
4. 用户选择稍后：staging 包保留，应用不退出，重启后可恢复 Downloaded 状态并重新校验。
5. 用户确认取消：只取消 snapshot 中相同 turn generation 的回合。
6. 取消 10 秒未完成：返回 blocker，不直接启动 NSIS。
7. staging 文件被修改：SHA-256 校验失败，禁止安装并删除该明确文件。
8. 两个窗口同时点击安装：只有一个 CAS 成功。

## 验收标准

- 任一活动回合存在时，NSIS `install()` 调用次数为 0，除非用户完成明确的强制退出确认。
- WaitingForIdle 不会自动取消 prompt、permission、tool 或 delegation。
- 更新后日志中的 `launching NSIS` 前必有 `install_gate_passed`，且 blocker count 为 0 或带用户强制确认标识。
- staging 包的版本、release identity、大小和 SHA-256 全部通过二次校验。
- shutdown cleanup 失败会被记录，但不会把未确认的活动回合伪装成已完成。

## 发布后验证

在新安装版分别执行“空闲安装”“下载期间启动 prompt”“等待自然完成”“用户取消活动回合”四组场景。按 version 和 release identity 检查状态转换日志，确认每条 `launching NSIS` 前都有唯一的 `install_gate_passed`；同时核对旧进程 PID 的退出时间，确保 WaitingForIdle 阶段桌面进程仍存活，且没有活动回合被自动标成成功。

## 授权与回滚

增加 WaitingForIdle/Downloaded 状态会修改共享 Rust/TypeScript update contract，实施前需要用户确认。

回滚时可关闭后台 staging，退化为“有活动回合时禁止开始下载”；无论如何不能回滚 install 前的 blocker 二次检查。
