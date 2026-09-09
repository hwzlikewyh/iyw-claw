# 门户用户中心、店铺、展厅与内容

资料来源：用户提供的 2026-09-09 接口详细版（线上前端包 + 部分登录态实测）。表中的“实测”是原资料的证据标记，不表示本次已调用。未明确的必填字段、类型、枚举、完整表单仍须结合实际页面请求确认。

调用入口：`fetch_iyw_url`。先读 [HTTP 约定](iyw-http.md)，每次提供 `description`；检查 HTTP 信封及业务 `body.code`。这里只提供接口资料，不创建 capability_id，不走 search/read/invoke。检索其他业务见 [接口索引](iyw-api-index.md)。

路径规则：本域所有表格路径加在 `https://www.iyw.cn/gateway` 后。主机自动注入当前登录态对应的 `iyuanwu_token` Cookie；不要提供 Cookie/token 参数。认证失败时恢复主机登录，不从浏览器提取凭证。

检索词：用户中心、VIP、银行卡、收藏分组、关注、客服、店铺、企业展厅、资质、帮助、首页、投诉、文档解析、上传视频。

主题、收藏和关注与图案网同名资源可能来自不同服务，使用当前列表返回的 ID。`/Message/*` 在原文有 `/gateway` 和 `/msgapi` 两种代理背景；默认本节 `/gateway`，只有观察到确切 `/msgapi` 请求后才用该代理，写操作不能失败后切代理重放。`/ai-agent/api/doc-parsing/parse` 只标“文件”，以及 `GetUploadVideo` 只标“表单”，没有 multipart 字段名/返回约定；先上传获取 URL，再依据实际契约决定能否以 JSON/form 调用，不能假造 file 字段。

```json
{"description":"查询我的店铺信息","url":"https://www.iyw.cn/gateway/Shop/GetMyShopInfo","body":{}}
```

银行信息和实名认证内容按任务最少披露；提交认证、投诉、消息、协议同意、店铺或企业申请均不是查询附带动作。

## 接口与参数

### 10.4 用户中心与其他

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `/User/GetUserInfo` | 用户信息 | 无 |
| POST | `/User/GetVipInfo` | VIP 信息 | 无 |
| POST | `/User/GetUserImgAuth` | 图片授权 | 无 |
| POST | `/User/GetUserImgSpaceInfo` | 图库空间 | 无 |
| POST | `/User/GetUserBankCard` | 银行卡 | 无 |
| POST | `/User/GetBankInfo` | 银行信息 | 无 |
| POST | `/User/SubBankCertify` | 提交银行认证 | 表单 |
| POST | `/User/GetCaiHongIdByUserId` | 彩虹号换用户 ID | `userId` |
| POST | `/User/AddOrCancelCollection` | 收藏 / 取消 | `targetId`、`type` |
| POST | `/User/AddOrEditCollectionGroup` | 收藏分组 | `groupId`、`name` |
| POST | `/User/DelCollectionGroup` | 删除分组 | `groupId` |
| POST | `/User/GetAllCollectionGroup` / `GetCollectionGroupList` | 收藏分组列表 | 无 / 分页 |
| POST | `/User/CollectCenter` | 收藏中心 | 分页、`type` |
| POST | `/User/GetMyFollow` | 我的关注 | 分页 |
| POST | `/User/SaveUserKeyword` | 保存搜索词 | `keyword` |
| POST | `/User/ContactCustomerService` | 联系客服 | `content` |
| POST | `/User/UserReadAndAgree` | 阅读协议 | `agreementId` |
| POST | `/Theme/GetThemeList` / `GetMyCollectTheme` / `GetThemeCreationList` | 主题列表 / 收藏 / 作品 | 分页 / 分页 / `themeId` |
| POST | `/Shop/GetMyShopInfo` | 我的店铺 | 无 |
| POST | `/Shop/GetIndustryInfo` | 行业信息 | 无 |
| POST | `/Shop/GetChannelInfo` | 渠道信息 | 无 |
| POST | `/Shop/IsExistenceNickName` | 昵称查重 | `nickName` |
| POST | `/Shop/SubitShopApply` / `SubitEnterprise` | 开店申请 / 企业提交 | 表单 |
| POST | `/Shop/FollowHall` | 关注店铺 | `shopId` |
| POST | `/Enterprise/GetEnterpriseHallDetail` | 企业展厅详情 | `enterpriseId` |
| POST | `/Enterprise/GetEnterpriseHallBasicInfo` | 企业基础信息 | `enterpriseId` |
| POST | `/Enterprise/SaveEnterpriseHallInfo` | 保存展厅 | 表单 |
| POST | `/Enterprise/SaveQualifyInfo` | 保存资质 | 表单 |
| POST | `/Enterprise/SaveRecommendCreationInfo` | 保存推荐作品 | `creationIds[]` |
| POST | `/Enterprise/GetPlayInfo` / `GetUploadVideo` | 播放信息 / 上传视频 | `videoId` / 表单 |
| POST | `/Enterprise/FollowEnterpriseHall` | 关注企业 | `enterpriseId` |
| POST | `/Enterprise/GetCreationConditionInfo` | 作品条件 | 筛选 |
| POST | `/Help/GetHelpList` / `GetHelpInfo` | 帮助列表 / 详情 | 分页 / `helpId` |
| POST | `/Help/GetDesignHelpList` / `GetDesignHelpInfo` | 设计帮助 | 分页 / `id` |
| POST | `/HomePage/GetHomeInfo` | 首页信息 | 无 |
| POST | `/HomePage/GetHomeRecommendList` | 首页推荐 | 分页 |
| POST | `/Support/GetBasicInfo` / `GetCategory` | 支持信息 / 分类 | 无 |
| POST | `/Support/GenPostPolicy` | 生成上传策略 | `fileName` |
| POST | `/Sys/AddComplainRecord` | 投诉记录 | 表单 |
| POST | `/Message/RecvMsgParser` | 消息解析 | `msgHandleCode`、`content` |
| POST | `/southwestFolk/southwestParent` / `searchWorks` | 西南民艺后端 | 分页 / `keywords` |
| POST | `/theme-activity/api/Trend/GetTrendDetail` | 趋势详情（门户侧） | `id` |
| POST | `/ai-agent/api/doc-parsing/parse` | 文档解析 | 文件 |
