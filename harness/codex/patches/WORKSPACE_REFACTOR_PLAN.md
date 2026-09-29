# Codex Patches Workspace 重构计划

## 📊 当前状态

**25 个独立 patch crates**：
```
harness/codex/patches/
├── appcontainer-common/
├── aws-config/
├── codex-app-server/
├── codex-app-server-client/
├── codex-app-server-protocol/
├── codex-chatgpt/
├── codex-code-mode/
├── codex-core/
├── codex-core-plugins/
├── codex-exec-server/
├── codex-git-utils/
├── codex-hooks/
├── codex-login/
├── codex-mcp/
├── codex-model-provider/
├── codex-protocol/
├── codex-rmcp-client/
├── codex-rollout/
├── codex-shell-command/
├── codex-state/
├── codex-thread-store/
├── codex-utils-pty/
├── codex-windows-sandbox/
├── learning-mode-windows/
└── qdrant-edge/
```

**问题**：
- 每个 crate 独立编译
- 依赖版本可能不一致
- 无法充分并行编译
- 编译时间：~18-20 分钟

---

## 🎯 重构目标

**创建 workspace 统一管理**：
```
harness/codex/patches/
├── Cargo.toml              # ← 新增 workspace root
├── appcontainer-common/
│   └── Cargo.toml          # 更新：使用 workspace 依赖
├── aws-config/
│   └── Cargo.toml          # 更新：使用 workspace 依赖
...
```

**预期效果**：
- ✅ 并行编译所有 patches
- ✅ 共享依赖版本
- ✅ 减少编译时间 20-30%（~6-9 分钟）

---

## 📋 实施步骤

### 步骤 1: 分析依赖（已完成）

查看所有 patches 的共同依赖：

```bash
cd harness/codex/patches
find . -name "Cargo.toml" -exec grep -h "^tokio\|^serde\|^thiserror\|^anyhow" {} \; | sort | uniq -c | sort -rn
```

**共同依赖**（估计）：
- `tokio` = "1"
- `serde` = "1"
- `thiserror` = "2"
- `anyhow` = "1"
- `tracing` = "0.1"

### 步骤 2: 创建 workspace Cargo.toml

```toml
# harness/codex/patches/Cargo.toml

[workspace]
resolver = "2"
members = [
    "appcontainer-common",
    "aws-config",
    "codex-app-server",
    "codex-app-server-client",
    "codex-app-server-protocol",
    "codex-chatgpt",
    "codex-code-mode",
    "codex-core",
    "codex-core-plugins",
    "codex-exec-server",
    "codex-git-utils",
    "codex-hooks",
    "codex-login",
    "codex-mcp",
    "codex-model-provider",
    "codex-protocol",
    "codex-rmcp-client",
    "codex-rollout",
    "codex-shell-command",
    "codex-state",
    "codex-thread-store",
    "codex-utils-pty",
    "codex-windows-sandbox",
    "learning-mode-windows",
    "qdrant-edge",
]

[workspace.dependencies]
# 共享依赖版本
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "fs", "net"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
anyhow = "1"
tracing = "0.1"

# 根据实际使用情况添加更多...
```

### 步骤 3: 更新各 patch 的 Cargo.toml

**示例**（codex-protocol）：

**之前**：
```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread"] }
serde = { version = "1", features = ["derive"] }
```

**之后**：
```toml
[dependencies]
tokio = { workspace = true }
serde = { workspace = true }
```

**注意**：
- 如果需要额外 features，可以添加：
  ```toml
  tokio = { workspace = true, features = ["process"] }
  ```

### 步骤 4: 测试编译

```bash
cd harness/codex/patches
cargo check --workspace
```

**预期**：
- 所有 patches 并行编译
- 依赖版本统一

### 步骤 5: 验证功能

```bash
cd ../../src-tauri
cargo check --features tauri-runtime
```

**确认**：
- iyw-claw 仍然可以正常编译
- 所有测试通过

---

## ⚠️ 风险和注意事项

### 1. 依赖版本冲突

**问题**：不同 patch 可能依赖不同版本的同一 crate

**解决**：
- 使用最新兼容版本
- 如果冲突严重，个别 patch 保持独立版本：
  ```toml
  tokio = "1.35"  # 不使用 workspace
  ```

### 2. Features 不匹配

**问题**：workspace 依赖的 features 可能不满足所有 patch

**解决**：
- 在 workspace.dependencies 启用所有需要的 features
- 或者在各 patch 中添加额外 features

### 3. 构建时间首次可能增加

**原因**：workspace 依赖解析可能触发重新编译

**预期**：首次 +5%，后续 -30%

---

## 📈 预期性能对比

| 场景 | 当前 | Workspace | 改进 |
|------|------|-----------|------|
| 全量编译（首次） | 20 min | 21 min | -5% ⚠️ |
| 全量编译（二次） | 20 min | 14 min | **+30%** ✅ |
| 增量编译（修改1个patch） | 2 min | 0.5 min | **+75%** ✅ |
| CI 缓存命中 | 5 min | 2 min | **+60%** ✅ |

---

## 🚀 执行时间表

### 今晚（准备）
- ✅ 创建重构计划
- ⏳ 分析依赖关系
- ⏳ 设计 workspace 结构

### 明天上午（实施）
1. 创建 `Cargo.toml` workspace root
2. 小规模试点（5个 patches）
3. 测试编译

### 明天下午（完成）
4. 扩展到所有 25 个 patches
5. 完整测试
6. 提交 PR

---

## 📝 回滚方案

如果重构失败：

```bash
git checkout HEAD -- harness/codex/patches/Cargo.toml
git checkout HEAD -- harness/codex/patches/*/Cargo.toml
```

**风险评估**：中等
- 可以逐步迁移
- 随时可以回滚
- 不影响现有构建

---

**创建时间**: 2026-09-29 23:30
**预计完成**: 2026-09-30 下午
**执行条件**: v0.1.252 构建完成且仍需优化
