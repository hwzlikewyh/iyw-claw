# IYW 图片工作流补充接口设计

## 目标

为 `iyw-image-workflows` 接入 `功能/补充搜索接口.txt` 中已经确认的图片工具和搜索接口，同时保持现有账号 token、dry-run、任务查询和敏感信息过滤约束。用户指定图片、上传图片或提供图片 URL 时，Skill 默认执行已封装的 `tool variation`；该本地别名映射到服务端 operation `g_tools_generate_image`，并将 payload 的 `toolName` 固定为 `variation`。

## 范围与架构

- `scripts/iyw_commerce.py tool <alias>`：固定别名映射到清单中的 commerce operation。请求体从 UTF-8 JSON 文件读取，接口路径只从内置白名单产生；不能由调用方注入 URL、prefix 或认证字段。
- `scripts/iyw_search.py search <alias>`：固定别名映射到 `tu.iyw.cn`、`www.iyw.cn` 或 `gateway.iyw.cn` 的只读接口；复用 `IywClient` 的 token 解析、POST、业务 code 校验和 dry-run。搜索结果递归过滤 token、Cookie、签名查询参数等敏感字段。
- `scripts/iyw_commerce_core.py` 增加各工具的 payload 校验。所有图片 URL 必须 HTTPS；本地文件仍先 `upload` 再使用返回的公开 URL；工具任务统一用现有 `task-get/task-wait` 查询。
- `SKILL.md` 增加工具目录、命令示例和图片输入优先级；专用工具只有用户明确要求时覆盖默认变款。

## 固定工具别名

`variation`、`extend`、`mix`、`pattern-apply`、`free-imitation`、`material-product`、`ip-apply`、`edit`、`outpaint`、`super-resolution`、`split-layers`、`separate-layers`、`enhance`、`extract-pattern`、`repeat-horizontal`、`convert`、`line-extraction`、`color-transfer`、`image-to-3d`、`video`、`model-scene`。

别名只能映射到清单中固定的 operation：`g_tools_generate_image`、`fission`、`erase`、`outpainting`、`SuperResolution`、`f_tools`、`EnhanceImage`、`convert`、`lineExtraction`、`ImageTo3D`、`videoGenerator`、`modelScene`。创建成功后只返回公开任务字段，内部模型、渠道、平台、价格等字段不回传。

## 搜索别名

`image`（图片搜索）、`catalog`（销售画册）、`report-list`、`report-detail`、`report-detail-tu`、`report-areas`、`report-years`、`report-recommendations`、`report-images`、`report-full`、`trend-list`、`trend-detail`、`trend-dict`、`tool-config`、`ip-list`、`ip-patterns`。所有请求使用 POST，并从账号 token 文件读取 token。

## 错误与验证

- 只有上游外层 `code == 1` 才算成功；HTTP 2xx 不代表业务成功。
- dry-run 不读取 token、不访问网络、不上传文件、不创建任务，只输出最终 URL 和 body。
- 收费创建请求不自动重试；等待超时只查询原 task ID。
- 测试覆盖工具别名映射、关键字段/枚举/HTTPS 校验、搜索 host/path、请求头只含 `token`、dry-run 输出，以及 Skill 的图片优先级文本。
