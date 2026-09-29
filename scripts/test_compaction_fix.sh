#!/bin/bash
# 测试上下文压缩超时修复

set -e

echo "=========================================="
echo "测试上下文压缩超时修复"
echo "=========================================="

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 1. 检查代码修改
echo -e "\n${YELLOW}[1/5] 检查代码修改...${NC}"

# 检查预算是否已修改为600秒
if grep -q "Duration::from_secs(600)" ./harness/codex/patches/codex-core/src/compact_request_budget.rs; then
    echo -e "${GREEN}✓ 压缩预算已更新为 600 秒${NC}"
else
    echo -e "${RED}✗ 压缩预算未更新${NC}"
    exit 1
fi

# 检查是否添加了with_budget方法
if grep -q "pub(crate) fn with_budget" ./harness/codex/patches/codex-core/src/compact_request_budget.rs; then
    echo -e "${GREEN}✓ 已添加可配置预算方法${NC}"
else
    echo -e "${RED}✗ 未找到可配置预算方法${NC}"
    exit 1
fi

# 检查是否添加了新的日志
if grep -q "compaction completed successfully" ./harness/codex/patches/codex-core/src/compact.rs; then
    echo -e "${GREEN}✓ 已添加增强日志${NC}"
else
    echo -e "${RED}✗ 未找到增强日志${NC}"
    exit 1
fi

# 2. 编译检查
echo -e "\n${YELLOW}[2/5] 编译检查...${NC}"
if cargo check --manifest-path ./Cargo.toml 2>&1 | tee /tmp/cargo_check.log; then
    echo -e "${GREEN}✓ 代码编译通过${NC}"
else
    echo -e "${RED}✗ 编译失败，请检查错误日志:${NC}"
    cat /tmp/cargo_check.log
    exit 1
fi

# 3. 检查日志文件
echo -e "\n${YELLOW}[3/5] 分析历史日志...${NC}"

LOG_DIR="D:/Users/iyw/Documents/WXWork/1688855060853829/Cache/File/2026-09"
LATEST_LOG=$(ls -t "$LOG_DIR"/iyw-claw*.log 2>/dev/null | head -1)

if [ -n "$LATEST_LOG" ]; then
    echo "分析日志文件: $LATEST_LOG"

    # 统计压缩超时次数
    TIMEOUT_COUNT=$(grep -c "compaction request budget exhausted" "$LATEST_LOG" || true)
    echo "历史压缩超时次数: $TIMEOUT_COUNT"

    # 查找最后一次超时
    LAST_TIMEOUT=$(grep "compaction request budget exhausted" "$LATEST_LOG" | tail -1 || true)
    if [ -n "$LAST_TIMEOUT" ]; then
        echo -e "${RED}最后一次超时:${NC}"
        echo "$LAST_TIMEOUT"
    fi

    # 统计压缩失败次数
    FAILED_COUNT=$(grep -c "Failed to run pre-sampling compact" "$LATEST_LOG" || true)
    echo "历史压缩失败次数: $FAILED_COUNT"

else
    echo -e "${YELLOW}⚠ 未找到历史日志文件${NC}"
fi

# 4. 生成压力测试建议
echo -e "\n${YELLOW}[4/5] 生成测试建议...${NC}"

cat > /tmp/compaction_test_plan.md << 'EOF'
# 压缩修复验证测试计划

## 1. 单元测试
```bash
# 测试RequestBudget基本功能
cargo test --package codex-core request_budget
```

## 2. 压力测试场景

### 场景A: 长对话上下文
- 创建一个包含大量消息的会话
- 累积超过 100k tokens 的上下文
- 触发压缩并观察日志

### 场景B: 网络不稳定模拟
- 使用网络代理工具（如 toxiproxy）
- 添加 1-2 秒的延迟
- 模拟间歇性网络故障

### 场景C: API 响应缓慢
- 模拟后端 API 响应时间 > 30s
- 验证重试机制
- 确认不会超过 600s 预算

## 3. 监控检查点

启动服务后，监控以下指标：

```bash
# 实时监控压缩日志
tail -f /path/to/log | grep -E "compaction (completed|failed|attempt)"

# 统计成功率
grep "compaction completed successfully" /path/to/log | wc -l
grep "compaction failed permanently" /path/to/log | wc -l

# 分析耗时分布
grep "compaction completed successfully" /path/to/log | \
  grep -oP 'elapsed_ms = \K\d+' | \
  awk '{sum+=$1; sumsq+=$1*$1; count++}
       END {print "平均:", sum/count, "ms";
            print "标准差:", sqrt(sumsq/count - (sum/count)^2), "ms"}'
```

## 4. 回归测试

确保修复不影响正常功能：
- [ ] 短对话正常压缩
- [ ] 中等对话正常压缩
- [ ] 压缩失败时用户收到友好提示
- [ ] 系统能够从压缩失败中恢复

EOF

echo -e "${GREEN}✓ 测试计划已生成: /tmp/compaction_test_plan.md${NC}"
cat /tmp/compaction_test_plan.md

# 5. 部署检查清单
echo -e "\n${YELLOW}[5/5] 部署前检查清单...${NC}"

cat << 'EOF'

┌─────────────────────────────────────────────────────────────┐
│                      部署前检查清单                          │
├─────────────────────────────────────────────────────────────┤
│ [ ] 代码已提交到版本控制                                     │
│ [ ] 已创建备份分支                                          │
│ [ ] 已通知相关团队成员                                       │
│ [ ] 准备好回滚方案                                          │
│ [ ] 测试环境验证通过                                         │
│ [ ] 生产环境部署时间窗口已确认                               │
│ [ ] 监控告警已配置                                          │
│ [ ] 文档已更新                                              │
└─────────────────────────────────────────────────────────────┘

EOF

echo -e "${GREEN}=========================================="
echo "所有检查完成！"
echo "==========================================${NC}"

echo -e "\n${YELLOW}下一步操作:${NC}"
echo "1. 运行完整编译: cargo build --release"
echo "2. 在测试环境部署并验证"
echo "3. 监控压缩性能指标 24 小时"
echo "4. 如果一切正常，部署到生产环境"
echo ""
echo -e "${YELLOW}快速编译命令:${NC}"
echo "  cd ./iyw-claw && cargo build --release"
echo ""
echo -e "${YELLOW}查看详细修复文档:${NC}"
echo "  cat ./COMPACTION_TIMEOUT_FIX.md"
