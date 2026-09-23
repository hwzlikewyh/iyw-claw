# 电商视频：生成、导演与任务管理

视频生成优先走本 Skill 的 `generate_iyw_image` / `fetch_iyw_url`。本篇覆盖产品演绎、参考视频复刻、自动导演、生成进度和历史管理；无需搜索或注册视频 capability_id。商品图片系列见 [商品套图](iyw-api-product-kits.md)。

来源：用户提供的《AI工作台接口梳理（电商视频 / 商品套图）》；证据为 `ai.iyw.cn/#/ecommerceVideo` 前端 `js/9750.23baca11.js`，以及原采集者对 `getCommerceTasks` 的真实抓包。其余生成接口为源码调用点证据，不表示本次已执行或端到端验证。

## 先选调用路径

| 任务 | 现有入口 | 选择依据 |
| --- | --- | --- |
| 已符合旧工具契约的单图视频 | `generate_iyw_image`，`type:video` | 单图 + prompt，ratio、4-15 秒整数 duration、mode=normal/hd；主机上传图片并等待任务 |
| 电商产品演绎、视频复刻、多图或 1-3 秒视频 | `fetch_iyw_url` -> `videoGenerator` | 按下方页面完整请求体；现有 video 封装只自动取 images 第一张，强制 mode 和 4-15 秒，不能直接表达这组页面契约 |
| 自动导演、智能复刻提示词 | `fetch_iyw_url` -> `videoAutoDirector` / `videoRemakeDirector` | 返回脚本；页面用 userHint，现有同名图片工具类型仍强制 prompt，未做 prompt -> userHint 映射 |
| 查询进度、视频历史、删除任务 | `fetch_iyw_url` | 保留真实 taskId；删除需属于用户当前授权 |

上述 fetch 路径是已知页面契约的现有网关入口，参数不匹配可在提交前判断，无需先制造工具失败。不要为通过旧校验补虚构 mode、丢参考图或改变时长。仅在用户指定其他服务，或适用的爱原物路径有明确不支持、不可用或失败证据时考虑其他路线；超时/未知结果不构成重新生成的依据。

不先查 `list_iyw_image_models`，它只提供图片模型。不要将视频页内部通道编号说成模型名称，也不要将“返回脚本”或“任务创建成功”描述为“视频已生成”。

## 请求约定与输入

本篇所有业务接口均为 `POST https://gateway.iyw.cn/ai-application/api/commerce/` + 下表操作名，JSON 请求体。fetch 传完整 URL、`method:POST`、具体动作 `description` 和 `body`；先读 [HTTP 约定](iyw-http.md)。

- 登录由主机处理；不读取 Cookie，不提供 token/tokenInfo。HTTP 成功后仍检查 `body.code === 1`。
- 图片/参考视频先复用可访问 URL。fetch 不上传文件，本地素材使用 [upload_iyw_file](iyw-upload.md)，上限 1 GiB；符合图片工具契约时由图片工具自行处理输入。
- 页面产品图最多 9 张，允许 webp/jpg/jpeg/png/gif/bmp；参考视频允许 mp4/mov/avi/mkv/webm，建议不超过 30 秒。页面的图片压缩（最长边 2048px、质量 0.9）属于前端行为，不代表工具会自动完成。
- 缺少所需商品图或复刻视频时取得实际素材，不编造 URL，也不自动改为无图生成。

## 视频生成

操作：`videoGenerator`。返回业务 `data.taskId` 后查询原任务。

| body 字段 | 页面类型与约束 |
| --- | --- |
| `duration` | number，1-15 秒，默认 15；旧工具只接受 4-15 的整数 |
| `ratio` | string，默认 `9:16`；`16:9/9:16/4:3/1:1/3:4/21:9` |
| `reference` | string，产品图 URL；多张用英文逗号连接，最多 9 张，不是 imageUrls 数组 |
| `prompt` | string，最终提示词最多 4000 字；product 场景包含成片风格说明 |
| `channelName` | 固定 `电商视频` |
| `toolName` | 固定 `video` |
| `modelChannel` | number，固定 `4`；仅为内部参数 |
| `scene` | `product`（默认，产品演绎）或 `remake`（复刻视频） |
| `productStyle` | 仅 product：`talk` 口播带货 / `showcase` 展示型 / `dance` 跳舞种草 / `interact` 互动使用 |
| `reference_videos` | 仅 remake：参考视频 URL 字符串 |
| `reference_video_durations` | 仅 remake：实际参考视频时长，秒数字符串；与目标 duration 区分 |

产品演绎示例（使用 fetch；替换示例地址和提示词再调用）：

```json
{
  "description": "生成商品口播视频",
  "url": "https://gateway.iyw.cn/ai-application/api/commerce/videoGenerator",
  "method": "POST",
  "body": {
    "duration": 15,
    "ratio": "9:16",
    "reference": "https://example.com/product-a.jpg,https://example.com/product-b.jpg",
    "prompt": "【成片风格：口播带货】成片类型：口播带货。人物面向镜头多段口播卖点，边讲边展示产品；无人物则产品特写+旁白。\n\n替换为用户要求及已确认的商品卖点",
    "channelName": "电商视频",
    "toolName": "video",
    "modelChannel": 4,
    "scene": "product",
    "productStyle": "talk"
  }
}
```

复刻时设置 `scene:remake`，省略 productStyle，传真实 `reference_videos` 与 `reference_video_durations`；保持这两个字段的下划线命名。参考视频时长不能用目标 duration 猜填。

## 导演与脚本

| 操作 | body | 返回 |
| --- | --- | --- |
| `videoAutoDirector` | `imageUrls:string[]`、`duration:number`（默认 15）、`ratio:string`（默认 9:16）、`userHint:string`、`style:string`（productStyle 值）、`platform:string`、`language:string` | `data.script` |
| `videoRemakeDirector` | `videoUrl:string`（http/https）、`imageUrls:string[]`、`duration:number`、`ratio:string`、可选 `userHint:string`（最多 500 字） | `data.script` |

自动导演的 platform 示例为 `tiktok/taobao/xiaohongshu/amazon/1688/temu`；language 按页面传中文名称：`中文/英文/日语/韩语/西班牙语/俄语`，不能混用套图的 en/zh 等编码。自动导演 userHint 的长度上限未在来源单独确认，不借用其他操作的限制。

只需脚本时交付 script；用户要求成片时，将脚本与用户要求整理为最多 4000 字的 prompt，再按目标 scene 提交一次 videoGenerator。导演调用本身不会完成视频生成，也不为已有完整提示词自动增加一轮导演调用。

## 进度、历史与删除

| 操作 | body | 结果与用途 |
| --- | --- | --- |
| `getCommerceTaskDetail` | `taskId:string` | `data.images[0].image` 是成品视频地址；结合 process 判断状态 |
| `getCommerceTasks` | `page:number`（从 1 开始）、`pageSize:number`（页面 10）、`commerceType:"video"`、`word/startDate/endDate:string`（无筛选为空串）、`processes:number[]`（页面 `[2,10,20]`） | `data.list` + `data.totalCount`；每条含 taskId/process/prompt/referenceImage/images/reason/createTime/finishTime/jsonData/stats |
| `removeTaskOrImage` | `taskId:string` | 检查 code/message；仅删除用户指定的任务，不为处理失败自动删除 |

查询示例（替换为生成结果中的 taskId）：

```json
{
  "description": "查询视频生成进度",
  "url": "https://gateway.iyw.cn/ai-application/api/commerce/getCommerceTaskDetail",
  "method": "POST",
  "body": { "taskId": "TASK_ID_FROM_RESPONSE" }
}
```

| process | 含义与下一步 |
| --- | --- |
| `1` / `2` | 排队 / 处理中；保留 taskId，继续查询 |
| `10` | 成功；取得非空视频 URL 后交付，cover 只作封面 |
| `20` / `30` | 失败；保留 taskId 和 reason/message，停止轮询 |
| 缺失或未知 | 不猜完成状态；查询详情或明确报告未知 |

手动查询可间隔约 5 秒，等待窗口参考页面 600 秒倒计时；这是本工作流的轮询建议，来源未确认视频页的精确轮询间隔。窗口结束仍处理中时交付任务状态与 ID，可通过历史继续查询，不自动再提交。

HTTP timeout_seconds（默认 60 秒）是单次请求上限，页面 600 秒倒计时是异步等待窗口，两者不同。网络异常、取消或响应不完整可能发生在提交之后；先查已有 taskId，缺失 ID 时按历史关键词/时间筛选核对，无法唯一确认就保留未知，不能换工具或服务重复生成。页面历史默认过滤不包含排队 1 和失败 30，列表缺失不证明任务不存在。

成功后用 `present_task_files` 交付已确认的视频 URL；保留原地址及所需查询参数。旧 video 工具没有完整 metadata.result，必要时用 fetch 重查详情获取原始视频 URL；不把非图片媒体声明为图片预览成功，也不通过 fetch 下载大视频（响应上限 2 MiB）。
