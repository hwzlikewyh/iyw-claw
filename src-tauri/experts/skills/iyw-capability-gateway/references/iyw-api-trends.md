# 趋势主题与报告详情

资料来源：用户提供的 2026-09-09 接口详细版（线上前端包 + 部分登录态实测）。表中的“实测”是原资料的证据标记，不表示本次已调用。未明确的必填字段、类型、枚举、完整表单仍须结合实际页面请求确认。

调用入口：`fetch_iyw_url`。先读 [HTTP 约定](iyw-http.md)，每次提供 `description`；检查 HTTP 信封及业务 `body.code`。这里只提供接口资料，不创建 capability_id，不走 search/read/invoke。检索其他业务见 [接口索引](iyw-api-index.md)。

路径规则：完整 URL 为 `https://gateway.iyw.cn/` + 表中服务路径；开头 `/` 只表示路径根，不改变域名。保留大小写和接口原始拼写。简称接口沿用该行左侧路径的父目录。GET 的参数放 `query`，POST 的参数放 `body`。

检索词：趋势主题、趋势报告、趋势匹配、market、trenderType、catalogue、关联图案、PDF。

`theme-activity` 详情的 `market=0` 表示全部；不要照搬 `ai-chat` 某列表将 0 转为 -1 的约定。`trenderType` 和 `trenderTypes` 分别为单值和数组，按实际业务需要使用。详情中的 `themePDFSource` 是资源来源，不代表文件已下载或已交付。

```json
{"description":"查询内销趋势主题","url":"https://gateway.iyw.cn/theme-activity/api/Trend/GetTrendList","body":{"keywords":"","market":1,"pageIndex":1,"pageSize":20}}
```

## 接口与参数

## 六、趋势主题（`theme-activity`）

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `theme-activity/api/Trend/GetTrendList` | 趋势主题列表 | `keywords`、`market`、`orderBy`、`pageIndex`、`pageSize` | `market` 内外销筛选 ✅ |
| POST | `theme-activity/api/Trend/GetTrendDetail` | 趋势报告详情 | `id`、`market`、`trenderType`、`catalogue`、`agentTrenderId` | ✅ 实测返回 `detailInfo{title,trendType,introduce,views,themePDFSource}` |
| POST | `theme-activity/api/Trend/GetReferenceIywTuList` | 趋势关联图案 | `id` | ✅ 返回 `data:[]` |

**扩展入参**（来自 AI 站趋势匹配，实际调用代码）：

```json
{
  "id": "主题ID",
  "agentTrenderId": 12,
  "catalogue": "主题子目录名",
  "market": 0,
  "trenderType": 3,
  "trenderTypes": [1, 2, 3]
}
```

- `market`：0 全部 / 1 内销 / 2 外销
- `trenderType`：0 全部趋势 / 1 AI 自动匹配趋势 / 2 展会报告 / 3 私有趋势 / -1 无主题
