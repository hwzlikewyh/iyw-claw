# 产品库与标签管理

资料来源：用户提供的 2026-09-09 接口详细版（线上前端包 + 部分登录态实测）。表中的“实测”是原资料的证据标记，不表示本次已调用。未明确的必填字段、类型、枚举、完整表单仍须结合实际页面请求确认。

调用入口：`fetch_iyw_url`。先读 [HTTP 约定](iyw-http.md)，每次提供 `description`；检查 HTTP 信封及业务 `body.code`。这里只提供接口资料，不创建 capability_id，不走 search/read/invoke。检索其他业务见 [接口索引](iyw-api-index.md)。

路径规则：完整 URL 为 `https://gateway.iyw.cn/` + 表中服务路径；开头 `/` 只表示路径根，不改变域名。保留大小写和接口原始拼写。简称接口沿用该行左侧路径的父目录。GET 的参数放 `query`，POST 的参数放 `body`。

本表简写 `userProduct/`、`userProductTags/`、`userProductTagsIndex/` 均补 `/ai-application/api/` 前缀。

检索词：产品管理、产品列表、产品详情、基础款、公开状态、产品标签、批量打标签、product、tag。

流程：先查产品和标签树，再使用真实 `productId`/`tagId`。`public:"-1"` 是列表筛选，不能用于发布；`isBasic:-1` 是基础款筛选。新增产品的 `categories` 来自类目 `categoryId`；不要把类目名称当 ID。批量标签原资料上限 10 个，未区分数组时两组都控制 <=10。消息推送仅在用户要求发送时执行。

```json
{"description":"查询基础款产品","url":"https://gateway.iyw.cn/ai-application/api/userProduct/getProductList","body":{"public":"-1","word":"","tagIds":[],"isBasic":1,"page":1,"pageSize":20}}
```

上传后使用 `upload_iyw_file` 返回的 `url` 作为产品 `imageUrl`；需要生成/改图时先调用 `generate_iyw_image`。上传、生成、保存产品是不同操作。

## 接口与参数

### 3.3 用户产品库（`userProduct` / `userProductTags`）

**全部 POST**，对应「产品管理」页。

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `userProduct/getProductList` | 产品列表 | `public`、`word`、`searchImage`、`tagIds[]`、`isBasic`、`page`、`pageSize` | `public` 公开状态（`"-1"` 全部）；`word` 关键词；`searchImage` 以图搜图；`tagIds` 标签筛选；`isBasic` 基础款（-1/0/1） ✅ 实测 |
| POST | `userProduct/getProductDetail` | 产品详情 | `productId` | |
| POST | `userProduct/addProduct` | 新增产品 | `type`、`imageUrl`、`tagIds[]`、`public`、`categories[]` | `categories` 映射为 `categoryId` 数组 ✅ |
| POST | `userProduct/updateProduct` | 更新产品 | `productId`、`type`、`imageUrl`、`tagIds[]`、`public` | |
| POST | `userProduct/deleteProduct` | 删除产品 | `productIds[]` | 支持批量 ✅ |
| POST | `userProduct/updatePublicStatus` | 改公开状态 | `productId`、`public`、`type` | `public=1` 公开 ✅ |
| POST | `userProduct/setBasicStatus` | 设为基础款 | `type`、`productIds[]`、`status` | `status=1` ✅ |
| POST | `userProduct/cancelBasicStatus` | 取消基础款 | `type`、`productIds[]`、`status:0` | ✅ |
| POST | `userProduct/sendProductMessage` | 产品消息推送 | `Equipment`、`platForm`、`Mobile`、`name`、`content`、`msgHandlecode` | 企微/客服消息 |
| POST | `userProductTags/getTagTree` | 标签树 | 无 | 返回 `tagTree[]` ✅ 实测 |
| POST | `userProductTags/createTag` | 新建标签 | `tagName` | |
| POST | `userProductTags/updateTag` | 更新标签 | `tagId`、`tagName` | |
| POST | `userProductTags/deleteTag` | 删除标签 | `tagId` | |
| POST | `userProductTagsIndex/addTagsToProducts` | 批量打标签 | `tagIds[]`、`productIds[]` | 最多 10 个 ✅ |
