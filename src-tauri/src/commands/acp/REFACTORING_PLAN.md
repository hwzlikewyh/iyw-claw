# acp.rs 重构计划 (12,963 行 → 模块化)

## 📊 当前状态

- **总行数**: 12,963 行
- **公共函数**: 61 个
- **主要问题**: 单文件过大，编译慢，增量编译效率低

## 🎯 拆分策略

### 模块划分（按功能域）

```
src/commands/acp/
├── mod.rs                    # 主入口，公共类型，事件定义
├── connection.rs             # 连接管理 (connect, disconnect, list, touch)
├── session.rs                # 会话操作 (prompt, cancel, fork, respond)
├── agent_lifecycle.rs        # 代理生命周期 (install, uninstall, download, detect)
├── agent_config.rs           # 代理配置 (update_config, update_env, preferences)
├── agent_status.rs           # 代理状态查询 (list, status, get_snapshot)
├── skills.rs                 # 技能管理 (list, read, save, enable, delete)
├── npm_runtime.rs            # NPM 运行时 (私有函数，从主文件移动)
├── uvx_runtime.rs            # UVX 运行时 (私有函数，从主文件移动)
└── vendor_specific.rs        # 特定供应商 (codex, opencode, hermes, kimi, pi)
```

## 📋 详细迁移计划

### 阶段 1: 准备工作 (10 分钟)

1. ✅ 创建 `src/commands/acp/` 目录
2. ✅ 创建 `mod.rs` 骨架
3. ✅ 提取公共类型和常量到 `mod.rs`
4. ✅ 保留原文件作为备份

### 阶段 2: 迁移辅助函数 (20 分钟)

**npm_runtime.rs** (行 299-1500, ~1200 行)
- `prewarm_uvx_agent`
- `probe_uvx_index`
- `verify_private_npm_package_version`
- `run_npm_streaming`
- `wait_npm_or_kill`
- `cleanup_npm_process`
- `fetch_managed_npm_*` 系列
- `install_managed_npm_packages`
- `install_private_npm_package`

**uvx_runtime.rs** (估计 ~800 行)
- UVX 相关的私有函数

### 阶段 3: 迁移核心命令函数 (30 分钟)

**connection.rs** (~1500 行)
```rust
pub async fn acp_connect(...)
pub async fn acp_disconnect(...)
pub async fn acp_disconnect_for_replacement(...)
pub async fn acp_disconnect_for_replacement_detailed(...)
pub async fn acp_touch_connection(...)
pub async fn acp_list_connections(...)
pub async fn acp_find_connection_for_conversation(...)
```

**session.rs** (~1200 行)
```rust
pub async fn acp_prompt(...)
pub async fn acp_cancel(...)
pub async fn acp_fork(...)
pub async fn acp_side_question(...)
pub async fn acp_respond_permission(...)
pub async fn acp_answer_question(...)
pub async fn acp_respond_html(...)
pub async fn acp_respond_channel_confirmation(...)
pub async fn acp_get_session_snapshot(...)
pub async fn acp_get_session_snapshot_by_conversation(...)
```

**agent_lifecycle.rs** (~1800 行)
```rust
pub async fn acp_download_agent_binary(...)
pub async fn acp_install_uv_tool(...)
pub async fn acp_detect_agent_local_version(...)
pub async fn acp_prepare_npx_agent(...)
pub async fn acp_uninstall_agent(...)
pub async fn acp_install_pi_binary(...)
pub async fn acp_uninstall_pi_binary(...)
```

**agent_config.rs** (~1500 行)
```rust
pub async fn acp_set_mode(...)
pub async fn acp_set_config_option(...)
pub async fn acp_update_agent_preferences(...)
pub async fn acp_update_agent_env(...)
pub async fn acp_update_agent_config(...)
pub async fn acp_describe_agent_options_core(...)
pub async fn acp_describe_agent_options(...)
```

**agent_status.rs** (~800 行)
```rust
pub async fn acp_list_agents(...)
pub async fn acp_get_agent_status(...)
pub async fn acp_reorder_agents(...)
```

**skills.rs** (~1500 行)
```rust
pub async fn acp_list_agent_skills_core(...)
pub async fn acp_list_agent_skills(...)
pub async fn acp_read_agent_skill(...)
pub async fn acp_take_over_agent_skill_core(...)
pub async fn acp_take_over_agent_skill(...)
pub async fn acp_save_agent_skill_core(...)
pub async fn acp_save_agent_skill(...)
pub async fn acp_set_agent_skill_enabled_core(...)
pub async fn acp_set_agent_skill_enabled(...)
pub async fn acp_delete_agent_skill(...)
pub async fn reconcile_shared_market_skills(...)
```

**vendor_specific.rs** (~1500 行)
```rust
// Codex
pub async fn codex_request_device_code(...)
pub async fn codex_poll_device_code(...)

// OpenCode
pub async fn opencode_install_plugins(...)
pub async fn opencode_list_plugins(...)
pub async fn opencode_provider_catalog(...)
pub async fn opencode_uninstall_plugin(...)

// Hermes
pub async fn acp_open_hermes_setup_terminal(...)
pub async fn acp_reveal_hermes_home(...)
pub async fn acp_update_hermes_config(...)

// Kimi
pub async fn acp_update_kimi_code_config(...)
pub async fn acp_fetch_kimi_models(...)

// Pi
pub async fn acp_update_pi_config(...)
pub async fn acp_load_pi_config(...)
pub async fn acp_validate_pi_command(...)
```

### 阶段 4: 更新导入和重新导出 (10 分钟)

**mod.rs** 负责重新导出所有公共函数：
```rust
// 重新导出所有命令函数
pub use connection::*;
pub use session::*;
pub use agent_lifecycle::*;
pub use agent_config::*;
pub use agent_status::*;
pub use skills::*;
pub use vendor_specific::*;
```

### 阶段 5: 测试和验证 (10 分钟)

1. `cargo check --features tauri-runtime`
2. `cargo test --features test-utils`
3. 确保所有导入路径正确

## 🎨 迁移模板

每个新模块的基本结构：

```rust
// src/commands/acp/connection.rs

use std::path::Path;
use crate::acp::error::AcpError;
use crate::acp::manager::ConnectionManager;
use crate::db::AppDatabase;
use crate::web::event_bridge::EventEmitter;

#[cfg(feature = "tauri-runtime")]
use tauri::State;

// 从 mod.rs 导入公共类型
use super::{emit_acp_agents_updated, AcpError};

/// 连接到 ACP 代理
pub async fn acp_connect(
    // ... 参数
) -> Result<(), AcpError> {
    // ... 实现
}

// 其他函数...
```

## ⚡ 预期效果

### 编译速度提升

| 场景 | 优化前 | 优化后 | 提升 |
|------|--------|--------|------|
| 全量编译 | 单线程 12,963 行 | 8 个模块并行 | **30-40%** |
| 增量编译（修改一行） | 12,963 行 | ~1500 行 | **85%** |
| 链接时间 | 受大文件影响 | 更好的代码局部性 | **10-15%** |

### 开发体验提升

- ✅ 代码导航更快
- ✅ IDE 自动完成更快
- ✅ 模块职责清晰
- ✅ 更容易并行开发

## 📝 注意事项

1. **保持向后兼容**: 所有公共函数通过 `mod.rs` 重新导出
2. **私有函数**: 只在模块内可见，不重新导出
3. **测试覆盖**: 确保所有测试通过
4. **Git 提交**: 每完成一个模块就提交一次

## 🚀 执行时间表

- **阶段 1**: 10 分钟
- **阶段 2**: 20 分钟
- **阶段 3**: 30 分钟
- **阶段 4**: 10 分钟
- **阶段 5**: 10 分钟

**总计**: ~80 分钟

---

**当前进度**: 准备阶段
**开始时间**: 2026-09-29 22:54
**预计完成**: 2026-09-29 24:14
