# Infinite Canvas Plugin Implementation Plan

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将固定版本的 `basketikun/infinite-canvas` 交付为 iyw-claw 可按需安装、无 tldraw、项目级持久化的原生 MCP Apps 插件，并完成 Cowart 常用能力迁移和 Fusion 发布。

**Architecture:** 插件使用一个 workspace 级 Node MCP runtime 和一个小于 8 MiB 的自包含 Widget。Widget 与 Agent 通过同一组 manifest-declared capabilities 读写 `canvas/infinite-canvas/`；宿主只提供通用 Plugin v2、授权、生命周期和 MCP Apps bridge。

**Tech Stack:** TypeScript 5.8、Node.js managed runtime、MCP SDK 1.12、Vite 7、React 19、Zod 3、pnpm 11、iyw-claw Plugin v2/Fusion Skill Market。

## Global Constraints

- 基线为 `iyw-claw@b43ebe63bbb49b3daa4db53fa247de924e81533a`。
- 上游固定为 `basketikun/infinite-canvas@ed013e8e5ce8ccab47cf2fc779f8e94555eb4c23`。
- 插件不进入原生安装包；artifact 只在用户安装插件时下载。
- runtime、Widget 和生产依赖不得包含 tldraw 或要求商业 license key。
- 正式路径不得依赖 `canvas.best`、loopback HTTP 服务或浏览器 IndexedDB。
- workspace 权限只覆盖 `canvas/infinite-canvas`；网络权限默认空。
- 所有 Widget 工具必须由 `.iyw-plugin.json` capability 声明；不绕过 HostGateway。
- 复用现有 100 MiB 产品上传上限；Widget 分块原始数据每块 128 KiB，保证 base64 JSON 低于 256 KiB bridge 上限。
- 按仓库 `AGENTS.md` 不新增或运行测试文件；每个任务使用 typecheck、build、verifier、静态调用链审查和真实客户端验收。
- 当前主工作区保持不变；所有实现位于 `feat/infinite-canvas-plugin-20260829` 独立 worktree。
- 新增函数 ≤50 行、文件 ≤300 行、嵌套 ≤3、位置参数 ≤3、圈复杂度 ≤10；注释默认简体中文。
- 本地提交可以按任务拆分；未经用户明确要求不 push、merge 或发布外部状态。

## Plan Set and Order

按以下顺序执行；每份子计划都产生可独立审查的交付物：

1. [Runtime Foundation](2026-08-29-infinite-canvas-runtime-foundation.md)
   - 插件目录、MIT/来源、manifest、共享 scene/ops、项目持久化、MCP 工具、resource 和 artifact verifier。
2. [Native Widget](2026-08-29-infinite-canvas-native-widget.md)
   - 固定上游源码、自包含 Widget、MCP Apps bridge、场景同步、媒体分块和全屏交互。
3. [Creative Parity](2026-08-29-infinite-canvas-creative-parity.md)
   - 图片生成/插入、标注编辑、HTML/Markdown/SVG、Slides 等价工作流。
4. [Migration and Release](2026-08-29-infinite-canvas-migration-release.md)
   - Cowart 非破坏迁移、许可证/包审计、Fusion artifact、安装客户端和发布验证。

## Cross-Plan Interfaces

Runtime Foundation 必须先产出以下稳定接口，后续计划不得另起同义协议：

```ts
export type CanvasScene = {
  schemaVersion: 1
  canvasId: string
  revision: number
  nodes: CanvasNodeData[]
  connections: CanvasConnection[]
  backgroundMode: "dots" | "lines" | "blank"
  showImageInfo: boolean
  viewport: { x: number; y: number; k: number }
  updatedAt: string
}

export type CanvasOperation =
  | { type: "add_node"; node: CanvasNodeData }
  | { type: "update_node"; nodeId: string; patch: Record<string, unknown> }
  | { type: "remove_node"; nodeId: string }
  | { type: "add_connection"; connection: CanvasConnection }
  | { type: "remove_connection"; connectionId: string }
  | { type: "set_viewport"; viewport: CanvasScene["viewport"] }
```

Widget、Agent、迁移器和导入导出全部使用 `CanvasScene` 与 `CanvasOperation`。资产引用统一为
`asset://<sha256>`；任何页面不得写绝对路径或把大体积 base64 留在 scene。

## Completion Gate

- [ ] 四份子计划全部完成，并且 staged/source/artifact 文件清单一致。
- [ ] `pnpm typecheck`、`pnpm build`、`pnpm verify` 和 `git diff --check` 成功。
- [ ] v2 manifest/runtime tool schemas/resource URI 通过 iyw-claw 安装时的真实 contract 校验。
- [ ] 正式 Windows 客户端完成安装、授权、打开、Agent 读写、刷新恢复、全屏、升级、禁用和卸载。
- [ ] 图片、标注、HTML、Markdown、SVG、Slides 和 Cowart 样例迁移有真实画面/文件证据。
- [ ] Fusion 普通用户目录可以发现、安装和再次使用插件。
- [ ] 只有上述证据齐全后，才把目标标记为完成。

## Plan Self-Review

- Spec 1-3（目标、宿主、方案）：Runtime Foundation Task 1-5 和 Native Widget Task 1 覆盖。
- Spec 4-6（包、能力、持久化）：Runtime Foundation Task 1-4 覆盖，tool/schema 命名一致。
- Spec 7-9（Widget、安全、错误）：Native Widget Task 1-6 与 Runtime Foundation Task 3-4 覆盖。
- Spec 10（创作等价）：Creative Parity Task 1-6 覆盖图片、标注、HTML、Slides 和失败保留。
- Spec 10-11（迁移、真实验收、回滚）：Migration and Release Task 1-6 覆盖。
- 已扫描所有计划，无未决占位标记或模糊步骤；跨计划统一使用
  `CanvasScene`、`CanvasOperation`、`CreativeRequestV1`、`asset://<sha256>` 和 stable capability IDs。
