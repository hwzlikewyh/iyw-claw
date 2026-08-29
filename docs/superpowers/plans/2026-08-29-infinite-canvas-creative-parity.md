# Infinite Canvas Creative Parity Implementation Plan

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在同一 Infinite Canvas 插件内补齐 Cowart 的图片生成、标注编辑、HTML 草稿和 Slides 核心工作流，使用户不再依赖 tldraw 插件。

**Architecture:** Widget 将用户意图、当前选区和 workspace 相对资产路径通过 `ui/message` 发送给当前 Agent；Agent 通过 Skills 调用现有模型能力，再用 Infinite Canvas stable capabilities 将结果写回同一 scene。标注和 Slides 使用内建节点，不增加外部服务或平行画布。

**Tech Stack:** MCP Apps `sendMessage`、IYW capability gateway、ImageGen/图片工作流、React/SVG/Canvas 2D、Infinite Canvas node plugin SDK。

## Global Constraints

- 先完成 Runtime Foundation 和 Native Widget 两份计划。
- 执行主计划的全部 Global Constraints。
- Widget 不直接持有模型 API Key；所有生成请求进入当前 Agent 的正常计费、权限和取消链路。
- 生成失败保留原节点和提示词，不自动重放或重复扣费。
- 资产通过 workspace 相对路径和 `asset://<sha256>` 传递，不在 `ui/message` 内发送大图 base64。

---

### Task 1: 持久化 Widget 选区和创作请求协议

**Files:**
- Create: `plugins/infinite-canvas/shared/creative-request.ts`
- Create: `plugins/infinite-canvas/widget/src/selection-sync.ts`
- Create: `plugins/infinite-canvas/widget/src/creative-request-client.ts`
- Modify: `plugins/infinite-canvas/runtime/src/tool-handlers.ts`
- Modify: `plugins/infinite-canvas/skills/infinite-canvas-edit/SKILL.md`

**Interfaces:**
- Produces: `CreativeRequestV1` 和可被 Agent 读取的 revision-bound selection。

- [ ] **Step 1: 定义稳定请求 envelope**

```ts
export type CreativeRequestV1 = {
  schemaVersion: 1
  requestId: string
  action: "image.generate" | "image.annotation-edit" | "html.generate" | "html.edit" | "slides.generate" | "slides.annotation-edit"
  canvasId: string
  revision: number
  selectedNodeIds: string[]
  prompt: string
  assetPaths: string[]
  targetNodeId?: string
}
```

`requestId` 只用于幂等和 UI 对应，不作为自动重试依据。

- [ ] **Step 2: 同步 selection**

Widget 选区变化防抖调用 `save_infinite_canvas_selection`，携带当前 scene revision；runtime
拒绝不存在的 node ID 或过期 revision。Agent Skill 在任何“这个/当前/选中”请求前调用
`get-selection` 并核对 revision。

- [ ] **Step 3: 发送安全 Agent 消息**

`sendCreativeRequest()` 发送一段用户可见中文摘要和一个 fenced JSON envelope；只包含相对
asset path，不包含 token、绝对路径或完整 scene。按钮进入 pending，收到 scene 新 revision
且目标节点出现后才变 success；取消只取消等待，不删除 Agent 已完成结果。

### Task 2: 接通图片生成和结果插入

**Files:**
- Modify: `plugins/infinite-canvas/skills/infinite-canvas-image-workflow/SKILL.md`
- Create: `plugins/infinite-canvas/widget/src/image-generation-panel.tsx`
- Create: `plugins/infinite-canvas/widget/src/generation-status.ts`
- Modify: `plugins/infinite-canvas/widget/src/widget-shell.tsx`

**Interfaces:**
- Consumes: `CreativeRequestV1(action="image.generate")`。
- Produces: 一个或多个 image nodes，保留 prompt、requestId 和生成状态。

- [ ] **Step 1: 写 Agent 图片工作流**

Skill 必须：读取 scene/selection；选择当前 Agent 可用的 IYW 图片能力或 ImageGen；将生成文件
保存到 workspace；调用 `write-asset(sourcePath)`；用 `apply-ops` 在选中节点右侧添加 image
node；成功后写 `status:"success"`。失败时只把占位节点改为 `status:"error"` 和安全错误码，
不自动再次生成。

- [ ] **Step 2: 实现 Widget 生成面板**

面板字段只包含 prompt、参考节点和输出数量；提交前创建 `status:"pending"` 的 config node，
再发送 request。重复点击同一 pending requestId 禁止；用户可明确点击“重试”生成新的
requestId，新结果不覆盖旧图。

- [ ] **Step 3: 处理多图布局**

Agent 根据原图 bounds 以固定间距向右排列，保持图片比例；多图使用 group 节点包裹并保存
primary image ID。Widget 不在结果到达前猜测图片尺寸。

- [ ] **Step 4: 静态审查计费与失败路径**

检查 pending、取消、Agent 断开、模型 402、部分成功和显式重试；任何失败都保留 prompt 和
成功图片，且没有后台自动 replay。

### Task 3: 实现图片标注层、扁平导出和按标注编辑

**Files:**
- Create: `plugins/infinite-canvas/widget/src/plugins/annotation/types.ts`
- Create: `plugins/infinite-canvas/widget/src/plugins/annotation/annotation-layer.tsx`
- Create: `plugins/infinite-canvas/widget/src/plugins/annotation/annotation-toolbar.tsx`
- Create: `plugins/infinite-canvas/widget/src/plugins/annotation/annotation-export.ts`
- Create: `plugins/infinite-canvas/widget/src/plugins/annotation/register.ts`
- Modify: `plugins/infinite-canvas/widget/src/plugins/register-builtins.ts`

**Interfaces:**
- Produces: `iyw:annotation-layer` node；export 返回 `asset://sha` 和 workspace relativePath。

- [ ] **Step 1: 定义 normalized annotation shapes**

```ts
export type AnnotationShape =
  | { id: string; type: "arrow"; from: Point; to: Point; color: string; label?: string }
  | { id: string; type: "rect" | "ellipse"; x: number; y: number; width: number; height: number; color: string }
  | { id: string; type: "text"; x: number; y: number; text: string; color: string }
  | { id: string; type: "freehand"; points: Point[]; color: string; width: number }
```

坐标相对关联图片归一化到 0..1；文本长度、点数和 shape 数在保存前验证。

- [ ] **Step 2: 实现 SVG overlay 编辑器**

选中图片后创建/复用关联 annotation-layer；工具栏支持箭头、矩形、椭圆、文字、自由画、
选择和删除。pointer capture 在 pointerup/cancel 全部释放；缩放只改变显示，不改 normalized
数据。

- [ ] **Step 3: 实现确定性扁平导出**

读取原图 object URL，在 Canvas 2D 按原图像素尺寸绘制图片和 SVG 等价 shapes；限制采用
现有图片路由的 20 MiB 输入和 1600 万总像素门禁，超限显示明确错误，不降质静默继续。
导出 PNG 通过分块 asset uploader 写入项目。

- [ ] **Step 4: 发送 annotation-edit**

请求包含原图 relativePath、扁平标注图 relativePath、文字标注摘要和目标 node ID。Agent
调用图片编辑能力，结果作为新 image node 放在原图右侧；旧图和 annotation-layer 保留。

### Task 4: 接通 HTML 草稿生成与编辑

**Files:**
- Create: `plugins/infinite-canvas/widget/src/plugins/html/html-actions.ts`
- Modify: `plugins/infinite-canvas/vendor/infinite-canvas-web/canvas-plugins/html/src/index.tsx`
- Modify: `plugins/infinite-canvas/skills/infinite-canvas-edit/SKILL.md`

**Interfaces:**
- Produces: HTML node metadata `{source, revision, requestId, status}`。

- [ ] **Step 1: 增加“生成网页/编辑网页”动作**

图片、文本和 HTML node 工具栏可发 `html.generate`/`html.edit`；请求包含选中内容摘要和 asset
路径。pending node 立即出现但 `source` 保留旧内容，失败时不清空。

- [ ] **Step 2: 写 Agent HTML 工作流**

Agent 生成单文件 HTML，禁止远程 script、iframe、表单提交和自动导航；使用 apply-ops 写入
target HTML node。编辑请求基于当前 source revision，过期时先读取最新 scene 再要求用户
确认，不覆盖并行编辑。

- [ ] **Step 3: 保持编辑/预览双态**

编辑器使用纯文本 code editor；保存才提交 operation。预览沿用 Native Widget 的严格二级
sandbox。导出 HTML 写 workspace 相对文件并返回可下载成果，不通过 data URL 打开。

### Task 5: 实现 Slides 等价节点和演示模式

**Files:**
- Create: `plugins/infinite-canvas/widget/src/plugins/slides/types.ts`
- Create: `plugins/infinite-canvas/widget/src/plugins/slides/slides-node.tsx`
- Create: `plugins/infinite-canvas/widget/src/plugins/slides/slides-toolbar.tsx`
- Create: `plugins/infinite-canvas/widget/src/plugins/slides/slides-presenter.tsx`
- Create: `plugins/infinite-canvas/widget/src/plugins/slides/slides-export.ts`
- Create: `plugins/infinite-canvas/widget/src/plugins/slides/register.ts`
- Modify: `plugins/infinite-canvas/widget/src/plugins/register-builtins.ts`
- Modify: `plugins/infinite-canvas/skills/infinite-canvas-edit/SKILL.md`

**Interfaces:**
- Produces: `iyw:slides` node，pages 可编辑、可演示、可导出、可发标注编辑请求。

- [ ] **Step 1: 定义 slide deck**

```ts
export type SlideDeck = {
  schemaVersion: 1
  title: string
  theme: { background: string; foreground: string; accent: string }
  pages: Array<{ id: string; title: string; html: string; notes: string }>
  activePageId: string
}
```

每页 HTML 使用与 HTML node 相同 sanitizer/sandbox ceiling；page ID 唯一，至少一页。

- [ ] **Step 2: 实现节点和演示器**

节点显示缩略图和页列表；presenter 在 Widget fullscreen 内切页，不打开新窗口。退出演示恢复
原 canvas viewport/selection；键盘监听在 unmount/teardown 时释放。

- [ ] **Step 3: 实现生成和标注编辑**

`slides.generate` 让 Agent 根据选中素材生成 deck JSON 并 apply-ops；annotation-edit 导出当前
页 PNG 和标注摘要，Agent 只更新目标 page，旧 page revision 不匹配时拒绝覆盖。

- [ ] **Step 4: 实现导出**

HTML 导出为自包含 deck；图片导出每页 PNG 并写入 `canvas/infinite-canvas/exports/<requestId>/`。
导出失败保留已成功页面并返回 manifest，用户可显式重试缺失页面。

### Task 6: 完成创作能力门禁

**Files:**
- Modify: `plugins/infinite-canvas/scripts/verify.mjs`
- Modify: `plugins/infinite-canvas/THIRD_PARTY_NOTICES.md`

- [ ] **Step 1: 扩展 verifier**

确认 annotation、HTML、Slides 节点均在 builtin registry；CreativeRequest action 与 Skill
分支完全对应；bundle 无模型 Key UI、远程 API endpoint、tldraw 或未声明 network domain。

- [ ] **Step 2: 运行静态门禁**

Run:

```powershell
pnpm --dir plugins/infinite-canvas typecheck
pnpm --dir plugins/infinite-canvas build
pnpm --dir plugins/infinite-canvas verify
git diff --check
```

Expected: exit 0；6 creative actions、3 builtin plugin families、0 external model calls。

- [ ] **Step 3: 静态调用链审查**

逐条审查生成、402、取消、显式重试、标注导出、HTML revision、Slides page revision、资源
释放和 teardown；记录无法由静态检查证明的真实模型/UI 验收项。

- [ ] **Step 4: 提交 parity**

```powershell
git add -- plugins/infinite-canvas
git commit -m "feat(plugin): 补齐 Infinite Canvas 创作工作流"
```
