# 分身生图契约

本契约仅用于用户明确要求 IYW 分身或多平台比稿。普通纯文生图、动漫和人物形象优先
使用内置 `imagegen`，由它从 Fusion 图片模型目录选择可生成模型。

分身生图使用 microModel batch 接口，不使用旧 Agent Image 路由，也不使用 commerce
任务查询接口。CLI 负责实时配置解析和 payload 构造，agent 只提供提示词。

## 已确认接口

| 阶段 | 方法 | 路径 |
| --- | --- | --- |
| 读取分身配置 | POST | `/platform/basic/dict/getByKeys` |
| 创建分身任务 | POST | `/ai-application/api/microModel/v2/batch` |
| 查询单个任务 | POST | `/ai-application/api/microModel/GetDetails` |

配置接口请求固定为：

```json
{
  "nameSpace": "COMMON",
  "keys": ["model_options"]
}
```

`model_options` 是 JSON 字符串。只选择标签以“分身”开头的配置，忽略垂直模型和
私有模型。默认只向一个平台下发并优先使用通道四；通道四不在实时配置中时，回退到
实时配置顺序中的第一个可用平台。遇到尚未内置精确默认参数的新分身时，必须在调用
batch 前失败。

## 创建

使用：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  fission-generate --prompt "产品设计草图" --wait-seconds 120
```

CLI 固定发送 `prompt`、`jsonData: null` 和已经确认的 `models` 数组。不要通过临时
JSON 文件覆盖 `platform`、`size` 或 `stats`。batch 是收费创建请求，只调用一次，
不得自动重试。

只有用户明确要求多平台比稿时才增加 `--compare-platforms`：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  fission-generate --prompt "产品设计草图" --compare-platforms `
  --wait-seconds 120
```

比稿模式向全部可用分身平台下发并将通道四排在第一位。通道四缺失时保持实时配置
顺序。默认模式和比稿模式都只调用一次 batch 创建接口。

创建成功后，保留响应中每个 `tasks[].data.taskId` 和 `groupId`。不要向用户暴露
`balance`、`micro`、`platform` 或其他内部路由字段。

## 查询与结果

每个 task ID 单独调用 `GetDetails`：

```json
{
  "taskId": "602862275132395520"
}
```

状态使用 `process`：`10` 为成功，`20` 或 `30` 为失败，其他值为排队或运行中。
只保留 `images[].image` 中的 HTTPS URL，并按 batch 任务顺序、任务内图片顺序返回。

等待超时时保留已有状态和原 task ID 供内部继续查询，但不向用户展示该 ID。继续使用
`fission-task-wait` 查询，不要创建替代 batch。对话内展示直接按 `SKILL.md` 的 Markdown 优先规则嵌入远程图片，不要为了
展示而下载；用户要求放入成果区时，另按 `SKILL.md` 的 `present_task_files`/完整网关
注册规则处理，不能用下载到工作区根目录代替成果区注册。

所有请求只发送 `token` 请求头。示例流量中的 `securitykey` 不是 CLI 契约，不得
发送。不得在命令、payload、日志或回复中写入 token。
