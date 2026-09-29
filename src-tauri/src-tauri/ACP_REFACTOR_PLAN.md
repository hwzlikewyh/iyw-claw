# ACP 模块拆分计划

## 🎯 目标

将 `src/commands/acp.rs`（12,963 行）拆分为多个专注的子模块，提高可维护性和编译速度。

## 📊 当前状态

- **src/commands/acp.rs**: 12,963 行，48 个公共函数
- **src/commands/acp.rs.backup**: 备份文件
- **子模块**: 已有 `runtime_timing.rs`

## 🗂️ 拆分方案

### 1. 保留在 `mod.rs` 中的核心内容

```rust
// src/commands/acp/mod.rs
- 模块声明和导出
- 核心类型定义
- 事件发射函数（emit_acp_agents_updated, emit_managed_tool_progress）
- 公共常量（ACP_AGENTS_UPDATED_EVENT, CODEX_MODEL_CATALOG_FILE）
```

### 2. 新子模块规划

#### 2.1 `version_validation.rs` - 版本验证和规范化
**行数预估**: ~150 行

```rust
pub(crate) fn validate_custom_version(version: &str) -> Result<String, AcpError>
pub(crate) fn normalize_version_candidate(version: &str) -> String
```

#### 2.2 `command_resolution.rs` - 命令查找和解析
**行数预估**: ~200 行

```rust
pub(crate) fn is_cmd_available(cmd: &str, runtime: &AgentRuntime) -> bool
pub(crate) fn resolve_command_on_path(cmd: &str) -> Option<PathBuf>
pub(crate) fn resolve_npx_command(package: &str, npm_prefix: Option<&str>) -> Option<PathBuf>
pub(crate) fn resolve_uvx_command() -> Option<PathBuf>
pub(crate) fn is_uvx_agent_spawnable(agent_type: &AgentType) -> bool
pub(crate) fn uvx_python_args(python: Option<&str>) -> Vec<String>
pub(crate) fn uvx_package_spec_for_version(...) -> String
```

#### 2.3 `agent_verification.rs` - 代理安装验证
**行数预估**: ~400 行

```rust
pub(crate) fn verify_agent_installed(...) -> Result<(), AcpError>
pub(crate) fn require_private_agent_storage_for_write() -> Result<AgentStoragePaths, AcpError>
async fn verify_private_npm_package_version(...) -> Result<(), AcpError>
async fn probe_uvx_index(...) -> Result<(), UvxPrewarmError>
```

#### 2.4 `agent_installation.rs` - 代理安装逻辑
**行数预估**: ~800 行

```rust
async fn prewarm_uvx_agent(...) -> Result<(), AcpError>
#[tauri::command] pub async fn acp_install_agent(...)
#[tauri::command] pub async fn acp_install_runtime_bundle(...)
pub(crate) async fn install_runtime_bundle_core(...)
pub(crate) async fn confirm_npm_agent_install(...)
pub(crate) async fn confirm_uvx_agent_install(...)
```

#### 2.5 `connection_lifecycle.rs` - 连接生命周期
**行数预估**: ~600 行

```rust
#[tauri::command] pub async fn acp_connect(...)
#[tauri::command] pub async fn acp_disconnect(...)
#[tauri::command] pub async fn acp_launch_agent(...)
pub(crate) async fn launch_agent_core(...)
```

#### 2.6 `session_operations.rs` - 会话操作
**行数预估**: ~800 行

```rust
#[tauri::command] pub async fn acp_send_prompt(...)
#[tauri::command] pub async fn acp_get_response(...)
#[tauri::command] pub async fn acp_fork_session(...)
#[tauri::command] pub async fn acp_list_sessions(...)
pub(crate) async fn send_prompt_core(...)
```

#### 2.7 `agent_config.rs` - 代理配置管理
**行数预估**: ~500 行

```rust
#[tauri::command] pub async fn acp_get_agent_config(...)
#[tauri::command] pub async fn acp_update_agent_config(...)
#[tauri::command] pub async fn acp_reset_agent_config(...)
pub(crate) async fn get_config_core(...)
pub(crate) async fn update_config_core(...)
```

#### 2.8 `agent_status.rs` - 代理状态查询
**行数预估**: ~400 行

```rust
#[tauri::command] pub async fn acp_get_connection_status(...)
#[tauri::command] pub async fn acp_list_agents(...)
#[tauri::command] pub async fn acp_check_agent_health(...)
pub(crate) fn get_status_core(...)
```

#### 2.9 `skills_management.rs` - 技能管理
**行数预估**: ~1000 行

```rust
#[tauri::command] pub async fn acp_list_skills(...)
#[tauri::command] pub async fn acp_get_skill_content(...)
#[tauri::command] pub async fn acp_sync_skill(...)
#[tauri::command] pub async fn acp_delete_skill(...)
pub(crate) async fn list_skills_core(...)
pub(crate) async fn sync_skill_core(...)
```

#### 2.10 `vendor_specific.rs` - 特定供应商逻辑
**行数预估**: ~600 行

```rust
// Codex 特定
pub(crate) async fn patch_codex_config(...)
pub(crate) fn create_codex_model_catalog(...)

// OpenCode 特定
pub(crate) async fn check_opencode_plugins(...)

// Hermes 特定
pub(crate) fn setup_hermes_python_env(...)
```

#### 2.11 `preflight_checks.rs` - 预检查逻辑
**行数预估**: ~300 行

```rust
#[tauri::command] pub async fn acp_run_preflight(...)
pub(crate) async fn run_preflight_core(...) -> PreflightResult
```

#### 2.12 `binary_management.rs` - 二进制管理
**行数预估**: ~400 行

```rust
pub(crate) async fn download_binary(...)
pub(crate) async fn verify_binary_checksum(...)
pub(crate) fn extract_binary_archive(...)
```

### 3. 已存在的模块

#### 3.1 `runtime_timing.rs` ✅
**当前**: 已存在
**功能**: 运行时计时统计

## 📋 拆分顺序（从简单到复杂）

### 阶段 1: 独立工具函数（无状态）
1. ✅ `runtime_timing.rs` - 已存在
2. 🔜 `version_validation.rs` - 版本验证（150 行，无依赖）
3. 🔜 `command_resolution.rs` - 命令查找（200 行，最小依赖）

### 阶段 2: 核心业务逻辑（有状态但独立）
4. 🔜 `agent_verification.rs` - 安装验证（400 行）
5. 🔜 `preflight_checks.rs` - 预检查（300 行）
6. 🔜 `binary_management.rs` - 二进制管理（400 行）

### 阶段 3: 供应商特定逻辑
7. 🔜 `vendor_specific.rs` - 供应商逻辑（600 行）

### 阶段 4: 复杂业务逻辑
8. 🔜 `agent_installation.rs` - 安装逻辑（800 行）
9. 🔜 `agent_config.rs` - 配置管理（500 行）
10. 🔜 `agent_status.rs` - 状态查询（400 行）
11. 🔜 `skills_management.rs` - 技能管理（1000 行）

### 阶段 5: 连接和会话（最复杂，最后）
12. 🔜 `connection_lifecycle.rs` - 连接生命周期（600 行）
13. 🔜 `session_operations.rs` - 会话操作（800 行）

## 🔧 技术约束

### Tauri 命令处理
- `#[tauri::command]` 函数必须在 feature gate `#[cfg(feature = "tauri-runtime")]` 内
- 每个 Tauri 命令需要一个 `_core` 版本供 Web handlers 使用
- 模式：
  ```rust
  #[cfg(feature = "tauri-runtime")]
  #[tauri::command]
  pub async fn acp_foo(state: State<'_, AppState>, ...) -> Result<T, AppCommandError> {
      foo_core(&state.db, &state.connection_manager, ...).await
          .map_err(Into::into)
  }
  
  pub(crate) async fn foo_core(
      db: &AppDatabase,
      conn_mgr: &ConnectionManager,
      ...
  ) -> Result<T, AcpError> {
      // 实际逻辑
  }
  ```

### 导出规则
- 所有 `pub(crate)` 函数在 `mod.rs` 中 `pub(crate) use` 重导出
- Tauri 命令在 `mod.rs` 中 `pub use` 导出（仅 tauri-runtime feature）
- 保持 API 稳定性：外部调用者不感知内部重构

## 📝 验证清单

每个拆分步骤完成后：
- [ ] `cargo check --features tauri-runtime` 通过
- [ ] `cargo check --no-default-features --features server-runtime` 通过
- [ ] `cargo test --features test-utils` 通过
- [ ] `cargo clippy -- -D warnings` 无警告
- [ ] 验证导入路径：确保所有 `use crate::commands::acp::*` 仍然有效

## 🎯 预期收益

### 编译速度
- **当前**: `acp.rs` 单文件 12,963 行，每次改动重编译整个文件
- **拆分后**: 13 个子模块，平均每个 500-1000 行
- **预期改善**: 增量编译时间减少 60-80%

### 可维护性
- 职责清晰：每个模块专注一个领域
- 易于测试：可以单独测试每个子模块
- 降低认知负担：每次只需关注 500-1000 行代码

### 并行编译
- 13 个模块可以并行编译
- 利用多核 CPU 加速构建

## ⚠️ 风险和注意事项

1. **循环依赖风险**: 
   - 仔细设计模块边界
   - 必要时使用 trait 或回调解耦

2. **Tauri 命令注册**:
   - 确保所有命令在 `main.rs` 中正确注册
   - 验证 `tauri::generate_handler!` 宏包含所有导出的命令

3. **测试覆盖**:
   - 拆分后可能暴露隐藏的 bug
   - 每个阶段完成后运行完整测试套件

## 📅 执行时间表

- **阶段 1**: 30-45 分钟（3 个简单模块）
- **阶段 2**: 60-90 分钟（3 个中等模块）
- **阶段 3**: 30-45 分钟（1 个供应商模块）
- **阶段 4**: 120-180 分钟（4 个复杂模块）
- **阶段 5**: 90-120 分钟（2 个最复杂模块）

**总计**: 5.5-8 小时

---

**创建时间**: 2026-09-29 23:58
**状态**: 📋 计划阶段
**下一步**: 开始阶段 1 - 拆分版本验证模块
