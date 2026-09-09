# 图片及产品接口摘要补充

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

生成/处理仍由 generate_iyw_image；本表中的列表、详情、收藏、删除、素材和产品管理全走 fetch_iyw_url。这里是补充篇的概括表，部分字段与其详细章节及旧已确认契约冲突。不能按这些摘要覆盖默认实现；具体取舍见 [冲突表](iyw-api-access-contracts.md)。

明确增加的参数变体：watermark-erase 可使用 target（或已有 mask）；classify-intent 可传 text（或旧 keys）；bleed-line 可传 bleed（或旧 size）；upscale/super-upscale 可传 scale；f-tools 可使用 toolName + imageUrls（或旧 content）。其余多版不同 ID/数组形状须有真实调用依据。

## 来源详细资料

## 八、电商工具 / 生图任务（`ai-application/api/commerce`、`microModel`，历史文档未细化）

### 8.1 commerce 工具接口（38 个，全部 POST）

基址 `https://gateway.iyw.cn`，前缀 `/ai-application/api/commerce`。调用方式高度统一：`{url: prefix + "/" + 动作, method:"post", data: 业务参数}`。

| 接口 | 功能 | 关键入参 |
| --- | --- | --- |
| `/commerce/erase` | 涂抹编辑 | `imageUrls[]`、`mask`、`toolName:"erase"` |
| `/commerce/blend` | 融合创款 | `imageUrls[]`、`prompt`、`toolName:"blend"` |
| `/commerce/mix`（由 `g_tools` 分发） | 多图融合 | `imageUrls[]`、`prompt`、`toolName:"mix"` |
| `/commerce/outpainting` | 智能扩图 | `imageUrls[]`、`ratio`、`prompt` |
| `/commerce/SeedEdit` | 指令编辑 | `imageUrls[]`、`prompt` |
| `/commerce/enhance`（`EnhanceImage`） | 画质增强 | `imageUrls[]`、`enhanceType` |
| `/commerce/upscaleImage` | 无损放大 | `imageUrls[]`、`scale` |
| `/commerce/SuperUpscale` | 超级放大 | `imageUrls[]`、`scale` |
| `/commerce/SuperResolution` | 高清修复 | `imageUrls[]` |
| `/commerce/lineart` | 线稿渲染 | `imageUrls[]` |
| `/commerce/lineExtraction` | 线稿提取 | `imageUrls[]` |
| `/commerce/imageModification` | 图片编辑 | `imageUrls[]`、`prompt` |
| `/commerce/bleedLine` | 出血线 | `imageUrls[]`、`bleed` |
| `/commerce/convert` | 格式转换 | `imageUrls[]`、`format` |
| `/commerce/ImageTo3D` | 转 3D 模型 | `imageUrls[]` |
| `/commerce/threeVisions` | 三视图 | `imageUrls[]` |
| `/commerce/fission` | 自由仿款 | `imageUrls[]` |
| `/commerce/vectorizeImage` | 矢量化 | `imageUrls[]` |
| `/commerce/f_tools` | 工具集统一入口（按 `toolName` 分发） | `toolName`、`imageUrls[]`、`prompt` 等 |
| `/commerce/g_tools` | 工具集统一入口（`fetch` 直连版，手写 `token` 头） | 同上 |
| `/commerce/g_tools_generate_image` | 通用生图 / A+ 单图编辑 | 见 §2.2，`securityKey:test` 头 |
| `/commerce/extraction` | 元素/图案提取 | `imageUrls[]`、`extractionType` |
| `/commerce/modelScene` | 模特场景图 | `imageUrls[]`、`scene` |
| `/commerce/watermarkEraser` | 消除水印 | `imageUrls[]`、`target` |
| `/commerce/videoGenerator` | 图转视频 | `imageUrls[]`、`prompt` |
| `/commerce/videoAutoDirector` | 视频自动导演 | `imageUrls[]`、`prompt` |
| `/commerce/videoRemakeDirector` | 视频重制导演 | `imageUrls[]`、`prompt` |
| `/commerce/detectImageGrid` | 检测图片网格 | `imageUrls[]` |
| `/commerce/buildExtractPrompts` | 生成提取提示词 | `imageUrls[]` |
| `/commerce/classifyCanvasIntent` | 画布意图识别 | `text`（`skipErrorToast:true`） |
| `/commerce/addCommerceCollect` / `removeCommerceCollect` | 收藏 / 取消收藏任务 | `taskId` |
| `/commerce/getCommerceTasks` | 我的任务列表 | 分页 + `type` |
| `/commerce/getCommerceTaskDetail` | 任务详情 | `taskId` |
| `/commerce/getCommerceTaskRecycleList` | 回收站列表 | 分页 |
| `/commerce/removeTaskOrImage` | 删除任务或单图 | `taskId`、`imageUrl` |
| `/commerce/searchTaskResult` | 批量查任务结果 | `task_ids[]`、`type`（商品套图为 `2`），超时 30 秒 |
| `/commerce/saveColor` / `extractColor` | 保存 / 提取色号 | `imageUrls[]`、色值数组 |

### 8.2 microModel 生图任务（全部 POST）

前缀 `/ai-application/api/microModel`，是全站生图主通道。

| 接口 | 功能 | 关键入参 |
| --- | --- | --- |
| `/microModel/v2/generate` | 提交生图任务（新） | `prompt`、`size`、`modelChannel`、`batchSize`、`toolName` |
| `/microModel/v2/batch` | 批量提交 | `prompts[]` |
| `/microModel/GetList` | 任务列表 | 分页 + 类型 |
| `/microModel/GetDetails` | 任务详情 | `taskId`（`securityKey:test`） |
| `/microModel/RequirementImageDetail` | 需求图详情 | `taskId` |
| `/microModel/task/detele` | 删除任务（接口名有拼写错误，原文如此） | `taskId` |
| `/microModel/Recycle` | 回收站 | 分页 |
| `/microModel/upscale` / `upscaleImage` | 放大 | `taskId` / `imageUrls[]` |
| `/microModel/variation` | 变款 | `taskId` |
| `/microModel/reset` | 重置任务 | `taskId` |
| `/microModel/checkImage` | 图片合规检测 | `imageUrls[]` |
| `/microModel/createNewChat` | 新建生图会话 | — |
| `/microModel/chat/history` | 生图会话历史 | `conversationId` |
| `/microModel/getMateial` | 取素材 | `taskId` |
| `/microModel/GetImageSegment` | 图像分割 | `imageUrls[]` |
| `/microModel/addCollect` / `removeCollect` / `collects` / `userCollections` / `deleteCollectImg` | 收藏相关 5 个 | `taskId` / `imageUrl` |
| `/microModel/stsToken`（GET） | 图片存储 STS | 无 |
| `/microModel/PreSignedUrl` | 上传预签名 URL | `fileName`、`fileType` |

### 8.3 用户产品库（`/ai-application/api/userProduct*`，全部 POST）

| 接口 | 功能 | 关键入参 |
| --- | --- | --- |
| `/userProduct/getProductList` | 产品列表 | 分页 + 分类 |
| `/userProduct/getProductDetail` | 产品详情 | `productId` |
| `/userProduct/addProduct` | 新增产品 | 产品字段 |
| `/userProduct/updateProduct` | 修改产品 | `productId` + 字段 |
| `/userProduct/deleteProduct` | 删除产品 | `productId` |
| `/userProduct/setBasicStatus` / `cancelBasicStatus` | 设为/取消基础款 | `productId` |
| `/userProduct/updatePublicStatus` | 公开/私有切换 | `productId`、`public` |
| `/userProduct/sendProductMessage` | 产品留言 | `productId`、内容 |
| `/userProductTags/getTagTree` | 标签树 | 无 |
| `/userProductTags/createTag` / `updateTag` / `deleteTag` | 标签增删改 | `tagId` / `name` / `parentId` |
| `/userProductTagsIndex/addTagsToProducts` | 给产品打标签 | `productIds[]`、`tagIds[]` |
