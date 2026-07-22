# 分身生图契约

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
私有模型。CLI 按实时配置顺序生成任务；遇到尚未内置精确默认参数的新分身时，
必须在调用 batch 前失败。

## 创建

使用：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  fission-generate --prompt "产品设计草图" --wait-seconds 120
```

CLI 固定发送 `prompt`、`jsonData: null` 和已经确认的 `models` 数组。不要通过临时
JSON 文件覆盖 `platform`、`size` 或 `stats`。batch 是收费创建请求，只调用一次，
不得自动重试。

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

等待超时时返回已有状态和原 task ID。继续使用 `fission-task-wait` 查询，不要创建
替代 batch。最终回复直接嵌入远程图片，不要为了展示而下载。

所有请求只发送 `token` 请求头。示例流量中的 `securitykey` 不是 CLI 契约，不得
发送。不得在命令、payload、日志或回复中写入 token。
