# 图片生成远程显示与完成态交付设计

## 背景

图片生成工具返回的是已上传到对象存储的公开 HTTPS URL。当前前端在图片没有
内嵌 base64 时仍拼接空的 `data:` URL，导致图片区域显示失败；同时，已完成的
助手回合把图片生成卡作为最终结果展示，过程没有按“只留交付成果”的方式收起。

## 目标

- 对带公开 `http/https` URI 的图片直接使用远程 URL 展示，不下载到工作区或
  转换成本地文件。
- 运行中的图片生成仍能显示占位和结果。
- 回合完成后，将 `generated-image` 归入可折叠的过程区域，最终可见区域只保留
  最终文本、显式展示图片和当前回复交付成果。
- 保持现有 `generate_iyw_image` 的
  `delivery.artifact.accepted[].path` 注册与成果面板，不凭空猜测路径。

## 方案

### 图片源解析

在 `GeneratedImagesBlock` 中按以下优先级解析图片源：

1. `iyw-claw://display-assets/<hash>`：继续通过现有桌面/服务端读取并创建 Blob
   URL。
2. `image.uri` 为公开 `http/https`：直接作为远程图片源，使用原生 `<img>`，避免
   动态对象存储域名受 Next Image 白名单限制。
3. 仅有 base64 数据：继续使用 `data:<mime>;base64,...`。

远程 URL 不执行下载、落盘或额外上传；如果未来某种交付工具要求本地文件，由
工具在交付阶段负责下载，前端图片组件不承担该职责。加载失败仍显示现有错误状态。

### 完成态分组

- 运行中：`generated-image` 仍作为可见结果参与简化视图，便于反馈生成进度。
- 已完成：`generated-image` 不再计入最终结果区，而是进入现有
  `ProcessDisclosure`。默认折叠策略、错误自动展开和过程计数继续复用现有逻辑。
- `displayed-image` 与 `CurrentReplyArtifactsPanel` 继续作为最终交付区域保留。

### 交付关联

沿用当前回复成果提取器：从图片生成工具回包的嵌套
`delivery.artifact.accepted` 中读取 URL，并通过当前会话/消息的成果查询匹配
已注册成果。若后端尚未返回已接受成果，不使用输入路径猜测替代。

## 错误处理

- 远程图片加载失败显示“图片渲染失败”，不显示空白或永久 loading。
- URL 成果预览继续使用现有 URL 预览路径；不因前端图片卡而下载文件。
- 成果列表加载失败保持现有错误和刷新机制。

## 验证

- 静态检查图片源优先级，覆盖远程 URL、内置 display-assets URI、base64 和失败
  分支。
- 静态检查完成态分组：完成后 `generated-image` 仅存在于过程折叠内容，交付面板
  仍由 accepted URL 驱动。
- 执行前端 lint 或类型检查（若仓库环境允许）；不新增测试文件。
