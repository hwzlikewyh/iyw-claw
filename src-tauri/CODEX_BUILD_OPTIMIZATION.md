# Codex 编译速度优化方案

## 📊 当前状况

从 Windows 构建日志可以看到，正在编译大量 codex 相关的包：

```
Compiling codex-protocol v0.156.1 (patches/codex-protocol)
Compiling codex-utils-pty v0.156.1 (patches/codex-utils-pty)
Compiling codex-windows-sandbox v0.156.1 (patches/codex-windows-sandbox)
... (25+ 个 patch crates)
Compiling codex-core v0.156.1 (patches/codex-core)
Compiling codex-api v0.156.1 (upstream)
...
```

**问题**：
- 25 个本地 patch crates
- 大量来自上游 Git 的依赖
- 串行编译（依赖关系复杂）

## ⚡ 优化方案

### 方案 1: 启用增量编译（最快见效）

**原理**：保留编译产物，只重新编译修改的部分

**实施**：
```yaml
# CI 环境变量
CARGO_INCREMENTAL: 1  # 当前是 0（禁用）
```

**效果**：
- 首次构建：无变化
- 二次构建：减少 50-70%

**问题**：
- 增加磁盘使用（~2-3 GB）
- 可能与 sccache 冲突

### 方案 2: 优化 codegen_units 和 LTO（已应用）

**当前配置**（来自构建日志）：
```yaml
CARGO_PROFILE_RELEASE_CODEGEN_UNITS: 64
CARGO_PROFILE_RELEASE_LTO: off
```

**问题**：
- `codegen_units=64` 编译快但链接慢
- `lto=off` 完全禁用优化

**已修复**（v0.1.251 正在测试）：
```toml
codegen_units = 16
lto = "thin"
```

### 方案 3: 使用 workspace 合并 patch crates（重构）

**当前问题**：
```
harness/codex/patches/
  ├─ codex-protocol/
  ├─ codex-utils-pty/
  ├─ codex-windows-sandbox/
  ... (25 个独立 crate)
```

每个 crate 都是独立编译单元，无法并行编译依赖它们的代码。

**优化方案**：
```toml
# harness/codex/Cargo.toml
[workspace]
members = [
  "patches/codex-protocol",
  "patches/codex-utils-pty",
  ...
]

[workspace.dependencies]
tokio = { version = "1", features = ["rt-multi-thread"] }
# 共享依赖版本
```

**效果**：
- 并行编译 patches
- 共享依赖版本，减少重复编译
- 预计提升 20-30%

### 方案 4: 缓存 codex 上游依赖（快速）

**当前**：每次从 GitHub 拉取
```toml
[patch."https://github.com/openai/codex.git"]
codex-core = { path = "../harness/codex/patches/codex-core" }
```

**优化**：
```yaml
# .github/workflows/release-tauri.yml
- uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index
      ~/.cargo/registry/cache
      ~/.cargo/git/db
      ~/.cargo/git/checkouts
    key: cargo-codex-${{ hashFiles('**/Cargo.lock') }}
```

**效果**：
- 首次构建：无变化
- 后续构建：减少 10-15 分钟

### 方案 5: 预编译 codex 依赖（中等难度）

**思路**：
1. 将 codex harness 编译为独立的静态库
2. iyw-claw 链接预编译的库，而不是重新编译源码

**实施**：
```toml
# harness/codex/Cargo.toml
[lib]
crate-type = ["staticlib", "rlib"]
```

**效果**：
- 减少编译时间 30-40%
- 需要管理预编译产物

## 🎯 推荐执行顺序

### 立即执行（今天）

1. ✅ **链接器优化**（已完成，v0.1.251 测试中）
   - `.cargo/config.toml` 配置 lld
   - 预期减少链接时间 50-70%

2. ⏳ **等待 v0.1.251 构建结果**
   - 验证 `codegen_units=16` + `lto=thin` 效果
   - 如果成功 < 90 分钟，暂缓其他优化

### 本周执行（如果 v0.1.251 仍然慢）

3. **启用 cargo workspace for patches**
   ```bash
   cd harness/codex
   # 创建 workspace Cargo.toml
   # 更新所有 patch crates
   ```
   - 预期减少 20-30%

4. **优化 CI 缓存策略**
   - 添加 codex Git checkout 缓存
   - 预期减少 10-15 分钟

### 下周执行（如果仍需优化）

5. **考虑预编译方案**
   - 评估预编译 codex 静态库的可行性
   - 预期减少 30-40%

## 📈 预期总效果

| 优化项 | 当前 | 优化后 | 节省 |
|--------|------|--------|------|
| 链接器 (lld) | ~15 min | ~5 min | **10 min** |
| codegen_units 16 + thin LTO | N/A | ~5 min | **5 min** |
| Workspace patches | ~20 min | ~14 min | **6 min** |
| 缓存优化 | ~15 min | ~5 min | **10 min** |
| **总计** | **~48 min** | **~25-30 min** | **~18-23 min** |

## ⚠️ 注意事项

1. **增量编译 vs sccache**
   - 两者可能冲突
   - 建议先测试链接器优化

2. **Workspace 重构风险**
   - 需要仔细测试所有 patch
   - 建议在单独分支进行

3. **CI 缓存大小**
   - codex Git checkout 缓存 ~500 MB
   - 需要评估 GitHub Actions 缓存限制

## 🔍 监控指标

构建完成后检查：
```bash
# 查看编译时间
cargo build --timings --features tauri-runtime

# 输出：target/cargo-timings/cargo-timing.html
```

关键指标：
- `codex-*` crates 总编译时间
- 链接时间 (`iyw-claw` final link)
- 总构建时间

---

**当前状态**: 等待 v0.1.251 构建完成验证链接器优化效果
**下一步**: 根据构建结果决定是否执行方案 3-5
