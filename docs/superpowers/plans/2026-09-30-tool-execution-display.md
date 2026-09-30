# 内置执行工具具体操作展示实施计划

> 按已确认的设计文档执行：`docs/superpowers/specs/2026-09-30-tool-execution-display-design.md`。

## 目标

让 `invoke_iyw_capability` 卡片在未展开时显示实际能力名称；已知能力使用现有翻译，动态或未知能力从安全的工具身份/能力 ID 生成可读标题。

## 任务

### 任务 1：扩展内置能力展示解析

**文件：** `src/lib/builtin-tool-display.ts`

- [x] 为展示结果增加可选动态名称字段。
- [x] 读取嵌套参数中的显式工具身份字段。
- [x] 对未知 `capability_id` 生成长度受限、去版本后缀的可读名称。
- [x] 保持已知能力映射和隐藏内部参数行为不变。

**验证：** 静态检查已知 ID、嵌套 ID、显式名称、未知 ID 和无能力信息的分支。

### 任务 2：接入标题本地化和动态回退

**文件：**

- `src/lib/tool-display.ts`
- `src/components/message/content-parts-renderer.tsx`
- `src/components/message/live-turn-activity.tsx`
- `src/i18n/messages/zh-CN.json`
- `src/i18n/messages/en.json`

- [x] 让通用标题 helper 支持带变量的动态能力翻译。
- [x] 聊天工具卡片优先显示动态能力标题。
- [x] 实时工具活动复用相同标题解析，避免运行中仍显示通用“调用工具”。
- [x] 添加中英文 `dynamicCapability` 文案。

**验证：** 检查现有静态能力、动态能力和非内置工具标题路径不回归。

### 任务 3：格式与静态验证

- [x] 对修改的 TypeScript/JSON 文件执行 Prettier 检查。
- [x] 对修改的前端文件执行 ESLint。
- [x] 执行 TypeScript 检查或项目可用的等价静态检查。
- [x] 执行 `git diff --check`，确认只包含本任务文件和既有工作区修改。
- [x] 清理所有临时验证文件，不运行或保留新增独立测试文件。
