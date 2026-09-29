# 构建优化执行计划

## 📊 当前状态（2026-09-29 23:25）

### v0.1.252 构建中 🔄
- **状态**: in_progress
- **开始时间**: 15:17 UTC (23:17 北京时间)
- **已运行**: ~10 分钟
- **包含优化**:
  - ✅ connection.rs 类型错误修复
  - ✅ 链接器优化 (lld)
  - ✅ codegen_units 64→16
  - ✅ lto off→thin

### 预期结果
- **Windows**: ~37-40 分钟（从 48 分钟减少）
- **macOS arm64**: 期望 <90 分钟（从 120+ 超时减少）
- **macOS x64**: 期望 <90 分钟（从 120+ 超时减少）

---

## 📋 优化路线图

### 阶段 1: 已完成 ✅

#### 1.1 链接器优化 ✅
```toml
# src-tauri/.cargo/config.toml
[target.aarch64-apple-darwin]
rustflags = ["-C", "link-arg=-fuse-ld=lld"]

[target.x86_64-apple-darwin]
rustflags = ["-C", "link-arg=-fuse-ld=lld"]

[target.x86_64-pc-windows-msvc]
linker = "rust-lld.exe"
```
**预期**: 链接时间减少 50-70% (~5分钟)

#### 1.2 编译优化 ✅
```yaml
# 已在 CI 中应用
codegen_units: 16  # 从 64 降低
lto: "thin"        # 从 off 启用
```
**预期**: 总时间减少 15-25% (~6分钟)

#### 1.3 关键修复 ✅
- connection.rs 类型错误修复
- 删除冲突的 acp 模块

---

### 阶段 2: 等待验证后执行 ⏳

#### 决策点：v0.1.252 构建完成后

**情况 A**: 构建成功 < 90 分钟 ✅
→ 阶段 1 优化生效，**暂缓阶段 2**，继续监控

**情况 B**: 构建失败或仍超时 ❌
→ 立即执行阶段 2 优化

---

### 阶段 2.1: CI 缓存优化（快速见效）

#### 优化内容
```yaml
# .github/workflows/release-tauri.yml
- name: Cache codex dependencies
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index
      ~/.cargo/registry/cache
      ~/.cargo/git/db
      ~/.cargo/git/checkouts
    key: cargo-codex-${{ runner.os }}-${{ hashFiles('**/Cargo.lock', 'harness/codex/patches/**/*.toml') }}
    restore-keys: |
      cargo-codex-${{ runner.os }}-
```

**预期效果**:
- 首次构建: 无变化
- 后续构建: 减少 10-15 分钟
- **风险**: 低

**工作量**: 1-2 小时

---

### 阶段 2.2: Workspace 重构 codex patches（中等难度）

#### 优化内容

**当前结构**（25 个独立 crate）:
```
harness/codex/patches/
  ├─ codex-protocol/Cargo.toml
  ├─ codex-utils-pty/Cargo.toml
  ├─ codex-windows-sandbox/Cargo.toml
  ... (22 more)
```

**优化结构**（workspace）:
```toml
# harness/codex/patches/Cargo.toml
[workspace]
members = [
  "codex-protocol",
  "codex-utils-pty",
  "codex-windows-sandbox",
  # ... 所有 patches
]

[workspace.dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde = { version = "1", features = ["derive"] }
# 共享依赖
```

**实施步骤**:
1. 创建 `harness/codex/patches/Cargo.toml`
2. 更新所有 patch 的 Cargo.toml 使用 workspace 依赖
3. 测试编译
4. 提交 PR

**预期效果**:
- 并行编译 patches
- 共享依赖版本
- 减少 6-9 分钟（20-30%）

**风险**: 中等（需要仔细测试）
**工作量**: 4-6 小时

---

### 阶段 3: 深度优化（长期）⏳

#### 3.1 预编译 codex 静态库

**思路**:
1. 将 codex harness 预编译为 `.a`/`.lib`
2. 缓存预编译产物
3. iyw-claw 直接链接，跳过源码编译

**预期效果**: 减少 28-32 分钟（70-80%）
**风险**: 高
**工作量**: 2-3 天

#### 3.2 acp.rs 模块拆分

**当前**: 12,963 行单文件
**目标**: 拆分为 8 个模块

```
src/commands/acp/
├── mod.rs              # 主入口
├── connection.rs       # 连接管理
├── session.rs          # 会话操作  
├── agent_lifecycle.rs  # 生命周期
├── agent_config.rs     # 配置
├── agent_status.rs     # 状态
├── skills.rs           # 技能
└── vendor_specific.rs  # 特定供应商
```

**预期效果**:
- 增量编译提升 85%
- 并行编译多个模块
- 总时间减少 10-20%

**风险**: 中等
**工作量**: 6-8 小时

---

## 🎯 执行时间表

### 今晚（等待 v0.1.252）

- ⏳ **23:25-23:55**: 等待构建完成（预计 30-40 分钟）
- 📊 **23:55**: 分析构建结果
- 🎯 **决策**: 是否执行阶段 2

### 明天（如果需要）

**上午**:
- 实施 CI 缓存优化（阶段 2.1）
- 测试并提交

**下午**:
- 开始 Workspace 重构（阶段 2.2）
- 小规模试点（5-10 个 patches）

**晚上**:
- 完成 Workspace 重构
- 触发测试构建

### 本周（如果仍需优化）

- 评估 acp.rs 拆分可行性
- 研究预编译方案

---

## 📈 预期总效果

| 阶段 | 优化项 | 当前 | 优化后 | 节省 | 状态 |
|------|--------|------|--------|------|------|
| **1** | 链接器 + 编译 | 48min | 37min | 11min | ✅ 测试中 |
| **2.1** | CI 缓存 | 37min | 27min | 10min | ⏳ 待执行 |
| **2.2** | Workspace | 27min | 21min | 6min | ⏳ 待执行 |
| **3** | 预编译 + 拆分 | 21min | 10min | 11min | 📋 长期 |

**最终目标**: 48分钟 → 10分钟（**减少 79%**）

---

## 🔍 监控指标

### v0.1.252 构建关键指标

**监控命令**:
```bash
export HTTP_PROXY=http://127.0.0.1:7890
gh run watch 36588999794 --repo hwzlikewyh/iyw-claw
```

**关注点**:
1. Windows 构建时间（目标 <45 分钟）
2. macOS arm64 是否超时（目标 <90 分钟）
3. macOS x64 是否超时（目标 <90 分钟）
4. 链接步骤耗时（期望减少 50%+）

### 成功标准

**阶段 1 成功**:
- ✅ Windows < 45 分钟
- ✅ macOS 都不超时（<90 分钟）
- ✅ 链接时间显著减少

**阶段 1 失败**:
- ❌ macOS 仍然超时
- ❌ Windows 无明显改善
- → 立即执行阶段 2

---

## 💡 教训和改进

### 本次遇到的问题

1. **过于激进的重构**
   - ❌ 在构建关键期创建 acp 模块
   - ❌ 导致模块冲突和编译失败
   - ✅ 应该先验证优化效果再重构

2. **正确的优先级**
   - ✅ 修复编译错误（最高优先级）
   - ✅ 应用低风险优化（链接器）
   - ✅ 验证效果后再考虑重构

### 未来改进

1. **渐进式优化**
   - 一次只做一个大改动
   - 每次改动都要验证效果
   - 保持代码随时可构建

2. **风险管理**
   - 低风险优化优先（链接器、缓存）
   - 中风险优化有备份（workspace）
   - 高风险优化单独分支（预编译）

3. **测试覆盖**
   - 本地先测试编译通过
   - 再提交到 CI
   - 准备好回滚方案

---

**更新时间**: 2026-09-29 23:25 UTC+8
**下次更新**: v0.1.252 构建完成后
**负责人**: Claude (AI Agent)
