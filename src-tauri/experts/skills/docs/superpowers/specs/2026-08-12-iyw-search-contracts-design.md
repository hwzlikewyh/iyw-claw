# IYW 搜索接口完整合同设计

## 背景

`iyw-image-workflows/scripts/iyw_search.py` 已登记补充接口文档中的 17 个查询别名，
但当前实现只固定 URL 并透传任意 JSON。调用者仍需了解原始接口字段，CLI 也无法在
请求前发现字段缺失、类型错误或敏感信息混入。现有文档还声称帮助输出包含 JSON
示例，实际并未提供。

补充文档后半部分的 Commerce operation 已由 `iyw_commerce.py tool <alias>` 覆盖，
本设计不重复实现 Commerce 接口，只完善图片、画册、报告、趋势、IP 和配置查询。

## 目标

- 为 17 个查询别名定义可执行的请求合同、默认值和字段校验。
- 保留 `search <alias> --input-file payload.json`，避免破坏现有调用。
- 增加 `example <alias>`，输出不含凭据的完整请求模板。
- 校验已确认的响应结构，并提供稳定、安全、适合 Agent 使用的结果。
- 继续固定 host、prefix、path 和 POST 方法，只从本机账号文件解析 token。
- 用参数化测试覆盖全部别名，并对智能图片搜索执行一次只读冒烟验证。
- 在用户明确要求智能搜索驱动趋势或主题设计时，形成搜索、推荐、主题拆页、图片工具
  路由和结果形态确认的完整 Agent 工作流。

## 非目标

- 不增加任意 URL、path、prefix 或 base URL 参数。
- 普通生图不强制串联搜索；用户明确要求智能搜索驱动设计时，搜索是必需前置步骤。
- 不重复实现 `iyw_commerce.py` 已覆盖的生图、编辑、扩图、3D 和视频接口。
- 不把补充文档中的 token、refresh token、Cookie 或请求示例凭据写入代码、测试、
  文档、日志或提交。

## 方案选择

### 方案 A：继续原样透传

改动最小，但无法满足完整接入：字段错误只能由服务端发现，也没有安全模板和稳定输出。

### 方案 B：为每个接口单独增加子命令

命令直观，但会产生大量重复 parser 和分支，并破坏当前统一别名入口。

### 方案 C：声明式合同注册表

在独立模块中为每个别名登记端点、示例、请求校验器和响应规范化器；现有 CLI 只负责
解析命令、读取文件和调度。本设计采用此方案，因为它保留兼容性，也能让 17 份合同
集中审计和参数化测试。

## 架构

新增 `scripts/iyw_search_specs.py` 和 `scripts/iyw_search_contracts.py`，包含以下职责：

- `SearchContract`：固定 endpoint、示例 payload 和字段 schema。
- `SEARCH_CONTRACTS`：17 个别名的唯一注册表。
- `validate_search_payload(alias, payload)`：拒绝错误 shape、未知字段和敏感字段，返回
  可发送的请求体。
- `normalize_search_response(alias, data)`：验证结构并过滤敏感字段、签名查询参数和
  不必要的内部数据。
- `example_payload(alias)`：返回深拷贝的安全模板，避免调用方修改注册表常量。

`scripts/iyw_search.py` 保留 `search` 命令，并新增 `example` 命令。`search` 的数据流为：

1. 从 UTF-8 JSON 文件读取 object 或 array。
2. 按别名调用请求校验器并补齐只在合同中明确的默认值。
3. 使用合同内固定 endpoint 创建 `IywClient`。
4. dry-run 时输出最终 POST URL 和请求体，不读取 token、不访问网络。
5. 实际调用时由 `IywClient` 验证 HTTP 状态及外层 `code == 1`。
6. 按别名校验并规范化 `data`，再包装为现有 `{ok, data}` CLI 信封。

## 查询合同

| 别名 | 请求 shape | 关键约束 | 响应 shape |
| --- | --- | --- | --- |
| `image` | object | `classify`/`exceptClassify` 为 ID 数组；文本或 HTTPS 图片至少一个非空；`market` 仅用于 `classify=51`；页码为正数 | 图片数组 |
| `catalog` | object | `name` 字符串；页码为正数；`timeRange` 字符串 | 画册数组 |
| `dict-industry` | array | 非空字符串 key 数组 | 字典 object |
| `report-areas` | object | `publish` 为整数 | 地区数组 |
| `report-years` | object | `publish` 为整数 | 年份数组 |
| `report-list` | object | 分页为正数；筛选项为数组；`title` 字符串 | 含 `records` 的分页 object |
| `report-detail` | object | `reportId` 为正整数或数字字符串；`type` 为整数或数字字符串 | 报告 object |
| `report-detail-tu` | object | `reportId` 为正整数或数字字符串 | 报告 object |
| `report-recommendations` | object | `reportId` 为正整数或数字字符串 | 含 `list` 的 object |
| `report-images` | object | 报告 ID 和分页合法；`imgType` 为整数 | 含图片列表/计数的 object |
| `report-full` | object | `reportId` 为正整数或数字字符串 | 完整报告 object |
| `trend-dict` | object | `keys` 为非空字符串数组 | 字典 object |
| `trend-list` | object | 分页为正数；关键词字符串；排序、市场、分类为整数 | 含 `items`、`totalCount` 的 object |
| `trend-detail` | object | `activityID` 为正整数或数字字符串 | 趋势详情 object |
| `tool-config` | object | `nameSpace` 非空；`keys` 为非空字符串数组 | 仅公开能力键 |
| `ip-list` | object | 分页为正数；关键词字符串；推荐和案例筛选为整数 | IP 列表 object |
| `ip-patterns` | object | 分页为正数；`ipId` 合法；`seriesId` 为非负数；`isDesign` 为布尔值 | 图案列表 object |

默认模板以补充文档中的已确认请求为准。校验器不猜测文档未证明的业务枚举；对于已证明
为整数但未证明取值范围的字段，只校验类型。所有分页大小必须大于零，并设置保守上限，
避免意外请求超大结果集。

## 结果与安全

- 请求体递归拒绝 `token`、`Cookie`、`Authorization`、`securityKey`、`tokenInfo`、
  password、secret、credential 和签名字段。
- 所有 URL 输入必须为 HTTPS，且不得包含签名、凭据或过期参数。
- 结果保留调用搜索和后续图片工作流需要的业务字段，但递归删除敏感字段，并清除 URL
  中的签名查询参数。
- 列表类结果统一返回 `items`；有服务端分页信息时同时返回 `total`、`page`、
  `page_size`。详情类返回 `item`。字典和配置类保留语义化 object。
- 服务端数据与已确认 shape 不符时返回 `invalid_response`，不得把异常结构包装成成功。
- `tool-config` 继续隐藏模型、渠道、价格和内部配置，只返回公开能力名。

## 文档与使用方式

`SKILL.md` 和 `references/tool-contracts.md` 将列出别名用途及以下标准流程：

```powershell
uv run --project $skillDir --python 3.13 python $searchCli example image
uv run --project $skillDir --python 3.13 python $searchCli `
  search image --input-file $payloadPath --dry-run
```

Agent 先用 `example` 获取模板，按用户条件填写，再 dry-run。不得从补充文档复制 Cookie
或 token，也不得向用户展示本机凭据。

## 智能搜索驱动的设计路由

用户明确要求先智能搜索图片和内容、再按趋势或主题设计时，Agent 先用趋势、图片及按需
报告别名搜索并推荐候选主题。第一次收费生成前，若用户没有明确说明，必须询问结果是
系列作品及其产品组成、4/6 宫格系列套组，还是单个/系列作品并形成企划案；推荐系列，
但不替用户决定。

每个主题只有一页素材且没有产品基准图时使用 `variation`，多页时按顺序使用 `mix`；
已有产品或设计基准图时优先 `extend`。用主题页得到种子作品后，系列结果继续使用
`extend`。固定别名和通用 `invoke` 都将三个工具的 `modelChannel` 强制设为 `2`。
`check-image` 在发送请求前拒绝签名 URL。显式搜索无结果时先调整搜索条件或改用用户
主题页，不直接退化为无依据生成。

## 错误处理

- 未知字段、错误类型、越界分页、非 HTTPS URL：`invalid_input`。
- HTTP、超时和业务码错误：沿用 `IywClient` 的错误分类和 `retryable`。
- 响应结构不符：`invalid_response`。
- 只读查询可在 `retryable: true` 时由上层决定是否重试；CLI 本身不隐藏重试行为。

## 验证

- 参数化测试证明 17 个别名均有固定 endpoint、示例、validator 和 normalizer。
- 每个示例执行 dry-run，断言 POST URL、请求体及无需 token。
- 覆盖必填字段、类型、分页边界、未知字段、敏感字段、HTTP URL 和签名 URL。
- 用代表性响应覆盖图片数组、普通数组、分页 object、详情 object、字典和配置过滤。
- 验证 CLI `example` 输出可重新作为对应 `search` 输入。
- 运行现有图片工具和 Skill 文档回归测试，确保 Commerce 行为不变。
- 若本机账号 token 可用，对 `image` 执行一次只读小分页搜索；只报告业务状态和结果
  数量，不输出凭据或整份响应。若外部服务不可用，明确将运行时验证标为未完成。
