# Task 06 缺陷台账

日期：2026-08-01

| # | 现象 | 根因 | 处置 | 状态 |
| --- | --- | --- | --- | --- |
| 1 | `src/i18n/messages/en.json` / `zh-CN.json` 被并行进程写入 `SkillMarketV2` 段，`toasts` 结构被移到顶层，语义损坏 | 多任务同时写同一 i18n 文件，注入相互覆盖 | 恢复为 HEAD，Task 06 前端组件改为内联文案，不再依赖新 i18n key | 已修复 |
| 2 | `binary_cache.rs` 删除 bundled uv/codex 后，`internet_tools.rs:95`、`commands/acp.rs:624-626` 仍调用 `bundled_uv_tool_paths` / `seed_bundled_uv_tools` / `BUNDLED_CODEX_ACP_VERSION` / `seed_bundled_codex_acp`，会编译失败 | 删除了外部仍引用的符号 | 恢复 `bundled_uv_tool_paths` 原实现，其余补 `Ok(false)` stub（no-op） | 已修复 |
| 3 | `runtime_bootstrap.rs` 的 `RuntimeBootstrapEvent.component` 为 `Option<&'static str>`，`emit(..., Some(tool_id), ...)` 传入非 `'static &str` | 生命周期标注错误 | 字段与 `emit` 签名改为 `Option<String>`，调用处 `Some(tool_id.to_string())` | 已修复 |
| 4 | `component.rs:25` 使用 `super::inventory`，但 `installer/` 下无 `inventory` 模块，实际位于 `version_center::inventory` | 模块路径写错 | 改为 `super::super::inventory` | 已修复 |
| 5 | 新增模块超行数上限：`resumable.rs` 518 行、`tools.rs` 421 行 | 功能密度高，未拆分 | 登记 known_risk，建议 Task 13 收尾拆分（binary_cache.rs 原 969 行属既有存量） | 已知风险 |
| 6 | `.github/workflows/release-tauri.yml` 仍校验 `uv uvx` sidecar 与 `resources/runtime` 归档，与新 `prepare-sidecars.mjs` 不一致，发布必失败 | Task 06 只改脚本，CI 属共享文件 | 生成 `T06-integration-requests.yaml` IR-003，交 Task 13 | 待接线 |
| 7 | `lib.rs` invoke_handler 未注册 `bootstrap_init_status` / `bootstrap_initialize` | lib.rs 属共享总入口 | 生成 IR-001，交 Task 13 | 待接线 |
| 8 | web router 未提供 bootstrap 状态/初始化路由 | router 属共享资源 | 生成 IR-002，交 Task 13 | 待接线 |
| 9 | 编译未验证 | 桌面端禁止本机 `cargo build/check/test/clippy` 与 `pnpm build` | 仅静态审查 + `git diff --check` + JSON/语法校验，如实声明 | 待远端 CI |

## 验证记录（本机静态）

- `git diff --check`：见最终检查输出。
- `tauri.conf.json` / i18n JSON：`JSON.parse` 通过。
- `prepare-sidecars.mjs`：`node --check` 通过。
- 跨模块符号：installer 各模块导出与引用逐一核对通过（含修复 #4 后路径）。
- 依赖：`reqwest` / `tokio` / `sysinfo 0.30` / `minisign-verify` / `sha2` / `futures-util` / `zip` 均在 `Cargo.toml` 已声明。
