# GitHub Actions 构建优化分析报告

## 当前问题

### 0.1.249 版本构建失败原因
1. **macOS x64/arm64、Linux x64 构建超时**（2小时限制）
2. **Linux arm64 编译失败**（29分钟，optional）
3. **只有 Windows 构建成功**（48分钟）

## 优化方案

### 方案 1：增加超时限制（快速修复）

修改 `.github/workflows/release-tauri.yml:53`：

```yaml
jobs:
  build:
    timeout-minutes: 180  # 从 120 改为 180（3小时）
```

**优点**：
- 立即生效，简单直接
- 给编译更多时间完成

**缺点**：
- 不解决根本问题
- GitHub Actions 计费时间增加

---

### 方案 2：优化编译配置（推荐）

#### 2.1 调整 codegen_units

**当前问题**：
```yaml
CARGO_PROFILE_RELEASE_CODEGEN_UNITS: "64"
```

`codegen_units=64` 会：
- ✅ 加快代码生成阶段（并行编译）
- ❌ 显著增加链接时间
- ❌ 增加最终二进制大小

**建议改为**：
```yaml
CARGO_PROFILE_RELEASE_CODEGEN_UNITS: "16"  # 或 "1" 用于 release
```

**理由**：
- Rust 编译的瓶颈通常在链接阶段，不是代码生成
- `codegen_units=16` 平衡编译速度和链接效率
- `codegen_units=1` 最优化，但编译最慢（适合最终 release）

#### 2.2 启用 thin LTO

**当前配置**：
```yaml
CARGO_PROFILE_RELEASE_LTO: "off"  # 完全关闭
```

**建议改为**：
```yaml
CARGO_PROFILE_RELEASE_LTO: "thin"  # 启用轻量级 LTO
```

**理由**：
- `thin` LTO 比 `off` 仅慢 10-20%，但二进制更小、性能更好
- `off` 完全不优化跨 crate 调用
- `fat` LTO 太慢（可能 2-3 倍时间），不适合 CI

---

### 方案 3：优化缓存策略

#### 3.1 增强 sccache 配置

当前已启用 sccache，但可以优化：

```yaml
- name: Setup sccache
  uses: ./.github/actions/setup-sccache
  
- name: Configure sccache limits
  shell: bash
  run: |
    sccache --set-config "cache.size=10G"  # 增加缓存大小
```

#### 3.2 Rust cache 优化

当前配置：
```yaml
- uses: swatinem/rust-cache@v2
  with:
    prefix-key: v1-desktop-app
    workspaces: ./src-tauri -> target
    shared-key: desktop-${{ matrix.target }}
    cache-bin: "false"
    cache-workspace-crates: "false"
```

**建议启用**：
```yaml
cache-workspace-crates: "true"  # 缓存工作区 crates
```

---

### 方案 4：并行化改进

#### 4.1 当前并行策略

```yaml
# release.yml:288
build_matrix: '{"include":[
  {"name":"macOS arm64","runner":"macos-latest",...},
  {"name":"Linux x64","runner":"ubuntu-22.04",...}
]}'
```

✅ **已经做得很好**：
- macOS arm64 + Linux x64 并行
- macOS x64 单独 job
- Windows 串行签名（硬件限制）

#### 4.2 Worker 编译并行

当前已启用：
```yaml
parallel_worker: true
```

✅ 这是好的设计，无需修改。

---

### 方案 5：增量编译优化

#### 5.1 启用增量编译（开发构建）

在 `src-tauri/Cargo.toml` 或 CI 环境变量中：

```toml
[profile.release]
incremental = false  # Release 构建保持关闭（推荐）
```

**注意**：Release 构建通常**不启用**增量编译，因为：
- 增加二进制大小
- 可能影响性能
- CI 环境缓存已足够

#### 5.2 减少编译的 features

检查 `src-tauri/Cargo.toml` 中是否有不必要的 features：

```bash
# 查看当前 features
cargo tree --features tauri-runtime | grep -E "^\w" | wc -l
```

---

## 推荐实施方案

### 阶段 1：立即修复（今天）

1. **增加超时到 180 分钟**
2. **调整 codegen_units 为 16**

```yaml
# .github/workflows/release-tauri.yml
jobs:
  build:
    timeout-minutes: 180  # +60 分钟
    env:
      CARGO_PROFILE_RELEASE_CODEGEN_UNITS: "16"  # 从 64 改为 16
```

**预期效果**：
- 链接速度提升 30-50%
- 构建应该能在 90-120 分钟内完成

---

### 阶段 2：中期优化（本周）

3. **启用 thin LTO**

```yaml
env:
  CARGO_PROFILE_RELEASE_LTO: "thin"  # 从 off 改为 thin
```

4. **优化依赖**

```bash
# 检查编译时间最长的 crates
cargo build --release --timings
# 查看 target/cargo-timings/*.html

# 考虑替换慢的依赖或启用其 feature flags
```

**预期效果**：
- 二进制大小减少 10-15%
- 运行性能提升 5-10%
- 编译时间增加 10-20%（可接受）

---

### 阶段 3：长期优化（下个版本）

5. **使用自托管 runner（可选）**

对于频繁发布的项目，考虑：
- 配置更强大的自托管 macOS/Linux runner
- 保留编译缓存在本地，避免每次下载

6. **Split 大型 crates**

如果 `src-tauri/src/` 中有单个文件超过 5000 行：
- 拆分为多个模块
- 提高并行编译效率

---

## 构建时间对比预测

| 配置 | macOS x64 | macOS arm64 | Linux x64 | Windows x64 |
|------|-----------|-------------|-----------|-------------|
| **当前** | 120+ min (超时) | 120+ min (超时) | 120+ min (超时) | 48 min ✅ |
| **阶段 1** | ~90 min | ~90 min | ~80 min | ~45 min |
| **阶段 2** | ~100 min | ~100 min | ~90 min | ~50 min |

**总结**：
- 阶段 1 应该能解决超时问题
- 阶段 2 提升质量（更小、更快的二进制），略微增加构建时间但在可接受范围内

---

## 其他发现

### Linux arm64 失败原因

从日志看，失败在 "Compile application alongside worker" 步骤。

**可能原因**：
1. ARM64 runner 资源不足（ubuntu-22.04-arm）
2. 交叉编译配置问题
3. 依赖库在 arm64 上不兼容

**建议**：
- 保持 `optional: true` 状态
- 单独排查 arm64 编译问题
- 或考虑放弃 Linux arm64 支持（使用较少）

---

## 立即行动清单

- [ ] 修改 `release-tauri.yml` 超时为 180 分钟
- [ ] 修改 `release.yml:288/304` 的 `codegen_units` 为 "16"
- [ ] 重新触发 0.1.249 构建
- [ ] 监控构建时间，记录改进效果
- [ ] 如果成功，考虑应用阶段 2 优化

---

生成时间：2026-09-29
相关版本：v0.1.249
