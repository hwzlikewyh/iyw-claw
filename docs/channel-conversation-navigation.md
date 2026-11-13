# 消息渠道原对话跳转

消息渠道详情中的“打开原对话”在桌面端调用宿主 `focus_conversation`，
恢复并聚焦对应工作区，通过 `workspace://focus-conversation` 事件打开原对话。
设置窗口保留当前页面，不导航到 `/workspace`。

- 本地设置返回 `main`，远程设置返回同一连接的 `remote-workspace-<id>`。
- 工作区在 `WorkbenchRouteProvider` 内挂载 `PetFocusBridge`，通过宿主 transport
  接收窗口事件。原对话已是当前标签时，也显式切回对话视图。
- 项目和标签尚未加载完成时，沿用现有等待逻辑；打开项目后再定位对话。
- 宿主调用失败时保留设置页，显示错误提示并记录目标项目、对话和错误。
- Web 模式继续通过原有对话链接导航。

定向验收：主窗口打开原对话、设置页保持原位、主窗口最小化后恢复、
已激活对话从其他工作区视图切回、远程设置返回对应远程工作区。

## 独立设置窗口的详情渲染

渠道详情复用工作区的消息渲染器。独立设置窗口通过
`ChannelRuntimeProviders` 补齐连接、委托、文件工作区和页面预览上下文；
工作区内的设置继续使用已有上下文。

消息链接和文件路径会调用 `useWorkspaceActions`，HTML 结果卡片会调用
`useInteractivePage`。缺少对应 Provider 时，渲染这些内容会抛错，
使设置页进入应用异常页面。

2026-11-13 验证：Provider 及其导入依赖链的 TypeScript 检查、定向 ESLint
和 diff 检查通过。全量类型检查被能力市场的 `capability-market-shell.tsx`
和 `toolbar.tsx` 中已有的 JSX 语法错误阻断；未运行自动化测试，
修复后的桌面客户端尚未打包并进行实机回归。
