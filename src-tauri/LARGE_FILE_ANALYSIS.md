# 大文件分析与优化策略

## 📊 当前状况

### 超大文件清单

| 文件 | 大小 | 行数 | 优先级 |
|------|------|------|--------|
| **src/commands/acp.rs** | 484K | 12,963 | 🔴 **最高** |
| src/acp/connection.rs | - | 9,315 | 🟠 高 |
| src/commands/mcp.rs | 160K | 4,628 | 🟡 中 |
| src/acp/manager.rs | - | 4,567 | 🟡 中 |
| src/acp/delegation/broker.rs | - | 4,376 | 🟡 中 |
| src/commands/folders.rs | 140K | 4,284 | 🟡 中 |

### 编译影响分析

#### 理论影响
- **增量编译**：修改 acp.rs 需要重编译 12,963 行
- **并行编译**：单个大文件无法并行
- **内存占用**：rustc 编译大文件内存峰值更高

#### 实际测量（TODO）
需要测量：
1. `cargo build` 总时间
2. 修改 acp.rs 后的增量编译时间
3. 各个模块的编译时间分布

```bash
cargo clean
cargo build --timings
# 查看 target/cargo-timings/cargo-timing.html
```

## 🎯 优化策略

### ⚠️ 风险评估

#### 高风险因素
1. **模块冲突**：acp.rs 与 acp/ 目录同名会导致编译错误
2. **循环依赖**：拆分不当可能引入循环依赖
3. **Tauri 命令注册**：拆分后必须更新 main.rs 的命令注册
4. **测试覆盖不足**：可能暴露隐藏的 bug

#### 已知问题
- **commit 7e230342**: 之前尝试拆分失败，原因是 acp.rs 和 acp/mod.rs 同时存在
- **临时解决方案**: 删除 acp/ 目录，保留 acp.rs

### 🚦 三阶段策略

#### 阶段 0：测量与验证（现在）
**目标**: 明确问题，避免盲目优化

**步骤**:
1. ✅ 确认 v0.1.253 构建成功
2. 🔜 运行 `cargo build --timings`
3. 🔜 分析编译时间分布
4. 🔜 测量 acp.rs 增量编译时间
5. 🔜 确定 acp.rs 是否真的是瓶颈

**输出**: 
- 编译时间报告
- 瓶颈识别
- 优化优先级排序

**决策点**:
- 如果 acp.rs 编译时间 < 5% 总时间 → **不优化**
- 如果 acp.rs 编译时间 > 15% 总时间 → **执行阶段 1**
- 如果其他模块更慢 → **优先优化其他模块**

#### 阶段 1：保守拆分（如果阶段 0 确认需要）
**目标**: 最小风险，提取完全独立的工具函数

**拆分范围**: 仅提取最独立、最简单的部分（预计 500-800 行）

1. **version_validation.rs** (~100 行)
   - `normalize_version_candidate()`
   - `sanitize_custom_version()`
   - 零依赖，纯函数

2. **constants.rs** (~50 行)
   - 所有 `const` 定义
   - 零依赖

3. **types_util.rs** (~150 行)
   - 小型辅助类型
   - 最小依赖

**验证步骤** (每个子模块后):
```bash
# 1. 创建新文件
# 2. 移动代码
# 3. 添加 mod 声明
# 4. 立即编译验证
cargo check --features tauri-runtime
cargo check --no-default-features --features server-runtime
cargo test --features test-utils
cargo clippy -- -D warnings

# 5. 如果失败 → 立即回滚
git checkout src/commands/acp.rs src/commands/acp/

# 6. 如果成功 → 提交
git add src/commands/acp/
git commit -m "refactor(acp): extract <module_name> (~N lines)"
```

**成功标准**:
- ✅ 所有检查通过
- ✅ 代码无冲突
- ✅ 减少 acp.rs 行数 5-10%

#### 阶段 2：激进拆分（仅在阶段 1 成功且有明显收益时）
**目标**: 完整模块化，大幅减小单文件大小

参考 `ACP_REFACTOR_PLAN.md` 的完整拆分计划。

**前提条件**:
- ✅ 阶段 1 成功
- ✅ 测量显示明显的编译时间改善
- ✅ 团队有充足时间进行测试

## 📋 当前决策

### 立即执行：阶段 0（测量与验证）
### 等待结果：v0.1.253 构建
### 暂缓执行：阶段 1、阶段 2

## 🔬 编译时间测量

### 命令

```bash
# 完整构建计时
cargo clean
cargo build --timings --features tauri-runtime

# 增量编译计时（修改 acp.rs）
# 1. 在 acp.rs 末尾添加一行注释
echo "// test change" >> src/commands/acp.rs
# 2. 测量重新编译时间
time cargo build --features tauri-runtime
# 3. 恢复
git checkout src/commands/acp.rs
```

### 预期结果

**场景 A**: acp.rs 不是瓶颈
```
总编译时间: 180s
acp.rs 编译: 8s (4.4%)
瓶颈: 依赖项编译 (80%)
结论: 不拆分 acp.rs，优化依赖项
```

**场景 B**: acp.rs 是瓶颈
```
总编译时间: 180s
acp.rs 编译: 35s (19.4%)
瓶颈: acp.rs 编译
结论: 执行阶段 1 拆分
```

## ⚠️ 回滚计划

### 如果拆分失败

```bash
# 1. 立即停止
# 2. 恢复到上一个稳定提交
git reset --hard HEAD^
# 3. 重新编译验证
cargo check --features tauri-runtime
# 4. 记录失败原因
echo "拆分失败: <原因>" >> REFACTORING_FAILURES.md
```

### 如果暴露隐藏 bug

```bash
# 1. 记录 bug
# 2. 评估严重性
# 3. 如果严重 → 回滚拆分
# 4. 如果轻微 → 修复 bug 并继续
```

## 📝 后续计划

### 短期（等待 v0.1.253 结果）
1. ⏳ 监控 v0.1.253 构建
2. ⏳ 如果成功 → 应用缓存优化
3. ⏳ 运行编译时间测量

### 中期（v0.1.253 成功后 1-2 天）
1. 🔜 分析编译瓶颈
2. 🔜 决定是否拆分 acp.rs
3. 🔜 如果拆分 → 执行阶段 1

### 长期（1-2 周）
1. 📅 如果阶段 1 成功 → 考虑阶段 2
2. 📅 定期测量编译时间
3. 📅 持续优化

---

**创建时间**: 2026-09-30 00:11
**状态**: 📊 分析阶段
**下一步**: 等待 v0.1.253 构建完成，然后进行编译时间测量
**风险级别**: 🟢 低风险（还未开始修改代码）
