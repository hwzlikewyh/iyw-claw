# L0-L3 权益、企业空间与费用

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

先根据任务选所需权益，不为了普通查询全量扫描组织、员工或账务。`authCode` 状态、剩余次数和菜单 capacity 是不同层级；前端按钮可见不证明后端已授权。具体用户当前权利以实时响应为准，历史点数表不作为当前报价。

L0 里短信发送仍有副作用；GET 删除员工/部门/角色和切换点数类型仍是写操作。用户仅要求查资料时不能执行这些接口。各域业务均用 fetch_iyw_url；生图/处理、文件上传例外走现有专用 MCP。

## 来源详细资料

## 一、按「可调用层级」重新分层

同一批接口，从「能不能调」的角度看，可以分成四层。这一层决定你接入时要不要先登录、要不要买会员。

### L0 · 完全公开（不传 token 也能拿到数据）

门户侧的展示类接口几乎都在这一层。实测不带 token 也能返回 `code=1`（个别会返回空列表）。

| 接口 | 请求类型 | 说明 |
| --- | --- | --- |
| `/tu-zp/api/Creation/GetCreationList` | POST | 作品列表（公开作品） |
| `/tu-zp/api/Creation/GetHomeWallList` | POST | 首页作品墙 |
| `/tu-zp/api/Creation/GetSearchCreationList` | POST | 作品搜索 |
| `/tu-zp/api/Creation/GetCreationArea` | POST | 按地区取作品 |
| `/tu-zp/api/HotCreation/GetList` | POST | 热门作品 |
| `/tu-zp/api/Classification/GetPopular` | POST | 热门类目 |
| `/tu-zp/api/Theme/GetRecommendThemeList` | POST | 推荐主题 |
| `/tu-zp/api/Ip/GetList` | POST | IP 列表 |
| `/tu-zp/api/ConceptDraft/GetList` | POST | 概念稿列表 |
| `/tu-zp/api/SouthwestCreation/GetIndex` | POST | 西南民艺首页 |
| `/theme-activity/api/Trend/GetTrendList` | POST | 趋势主题列表 |
| `/theme-activity/api/Trend/GetTrendDetail` | POST | 趋势报告详情 |
| `/theme-activity/api/Trend/GetReferenceIywTuList` | POST | 趋势关联图案 |
| `/exhibition/report/queryList`、`/detail`、`/getAreaList` | POST | 展会报告 |
| `/HomePage/GetHomeInfo`、`/GetHomeRecommendList` | POST | 首页数据 |
| `/Help/GetHelpList`、`/GetHelpInfo` | POST | 帮助中心 |
| `/Support/GetBasicInfo`、`/GetCategory` | POST | 支持信息 |
| `/account-search/basic/city/listSimplify`、`/CHList` | POST | 城市列表 |
| `/iyw-fusion-api/app-updates/v1/latest` | GET | 桌面客户端版本检查（完全免登录，`skipDefaultToken:true`） |
| `/Account/GetSmsCode` | POST | 获取短信验证码（登录前） |
| `/user-service/sms/send` | POST | 获取短信验证码（含 `Equipment`、`platForm`） |

这些接口免登录只是「能拿到公开数据」，不等于能拿到你的私有数据；带 token 时会返回带收藏/授权状态的版本。

### L1 · 需登录（有 token 即可，不校验会员权益）

| 接口 | 请求类型 | 说明 |
| --- | --- | --- |
| `/user-service/user/getMyInfo` | POST | 用户信息 + 菜单树 + 机构列表 ✅ 实测 |
| `/user-service/user/getCurrentUserInfo` | POST | 当前用户 |
| `/user-service/user/getInfo` | POST | 基础资料 |
| `/user-service/user/isOldUser` | POST | 是否老用户 |
| `/user-service/org/getOrgAvatar` | POST | 机构头像 |
| `/user-service/org/focus/save` | POST | 保存机构关注 |
| `/user-service/menu/menuList` | GET | 权限菜单树 |
| `/user-service/merchant/getCurrentOrg` | GET | 当前机构 |
| `/user-service/employee/list` | POST | 员工列表 |
| `/user-service/employee/employeeDeptList` | POST | 员工部门列表 |
| `/user-service/dept/list`、`/listTwo` | POST | 部门列表 |
| `/user-service/role/list` | POST | 角色列表 |
| `/user-service/employeePoints/getAiPoints` | POST | 可用点数 ✅ 实测 `{availablePoints:141850}` |
| `/user-service/employeePoints/recordPage` | POST | 点数消耗明细 ✅ 实测 |
| `/user-service/employeePoints/typeList` | POST | 消耗类型清单 ✅ 实测 |
| `/user-service/employeePoints/getAiPointsType` | GET | 点数类型枚举 |
| `/user-service/employeePoints/page` | POST | 成员点数汇总 |
| `/ai-agent/api/conversation/*` | POST | 原助理会话全套 |
| `/ai-agent-new/api/agent/*` | GET/POST/DELETE | 新版 Agent 会话全套 |
| `/ai-application/api/microModel/*` | POST | 生图任务全套 |
| `/ai-application/api/commerce/*` | POST | 电商工具全套（部分工具还要额外权益，见 L2） |
| `/ai-application/api/userProduct/*` | POST | 用户产品库全套 |
| `/ai-chat/api/*` | POST | AI 对话、客户需求、设计稿 |
| `/ai-application/api/image2pdfCart/*` | POST | PDF 购物车 |
| `/member/User/GetSaleId` | POST | 取销售 ID |
| `/platform/basic/dict/getByKeys` | POST | 数据字典 |
| `/platform/oss/securityToken`、`securityNewToken` | POST | OSS STS 令牌 |
| `/ai-application/api/microModel/stsToken` | GET | 图片存储 STS |
| `/ai-application/api/microModel/PreSignedUrl` | POST | 上传预签名 URL |
| `/sso/api/sso/web/renewToken` | GET | 刷新登录态 |
| `/sso/api/sso/web/discardToken` | POST/GET | 退出登录 |
| `/account-admin/revise/switchWorkspace` | POST | 切换空间 |
| `/account-admin/revise/phoneCaptcha` | POST | 手机验证码 |
| `/account-search/web/event/getMyEventCode` | POST | 取事件码 |

### L2 · 需会员权益（登录 + `authCode` 校验通过）

AI 站在启动时调一次 `member/api/v2/MemberAuth/GetAuthList`，把结果存进 Vuex 的 `authObject`，之后用 `hasAuth([...])` 判断。实测传参固定为这 15 个码：

```json
{ "authCodes": ["T33","T34","A1","A2","T29","I6","I1","I2","I5","I8","I3","I10","I11","S2","I12"] }
```

判定规则（源码 `hasAuth`）：

- 对 `I1、I2、A2、I5、T33、I8、I10、I11、I12`：只看 `status == 1`（开通即可，不消耗次数）
- 其余（含 `T34、T29、I6`）：要求 `status == 1 && remain > 0`（要还有剩余次数/点数）

#### authCode 完整映射表（从源码逐个反查得到）

| authCode | 权益名 | 控制的功能 | 判定依据 | 证据位置 |
| --- | --- | --- | --- | --- |
| `T33` | 开通类权益 | 会员权益总开关之一 | `status==1` | `ai_app.js` `hasAuth` 白名单 |
| `T34` | AI 点数余额 | 全局余额，`balance = T34.remain`；AI 概念稿采纳「每张消耗 50 点」 | `status==1 && remain>0` | `ai_app.js` `SET_BALANCE(t["T34"]?.remain)` |
| `T29` | AI 概念稿查看/定制 | 查看 AI 概念稿详情、申请定制专属 AI 稿、方案生成扣点 | `status==1 && remain>0` | `ai_6629`、`ai_7844`、`ai_8633` |
| `I1` | 开通类 | 在 `hasAuth` 白名单中 | `status==1` | `ai_app.js` |
| `I2` | 开通类 | 在 `hasAuth` 白名单中 | `status==1` | `ai_app.js` |
| `I3` | 私有模型训练 | 申请模型定制服务时的判定文案「客户有/没有私有模型训练的权益(I3)」 | 直接读 `status` | `ai_268` `handleSubmit` 工单文案 |
| `I5` | 模特模型训练 / 模特与场景图 | 访问「模特与场景图」、AI 试衣、模特模型定制申请（工单码 `C-MTYCJT`、`C-DZSYMX`） | `status==1` | `ai_6946` `checkAuth()` |
| `I6` | AI 概念稿生成次数 | 按「方案数×图片数」扣 `I6.remain` | `remain >= 方案数×图片数` | `ai_8633` `handleSubmit` |
| `I8` | 私有趋势 | 私有趋势引用（工单提示「您当前账号未有私有趋势权益」） | `status==1` | `ai_3019` `getAuthData` |
| `I10` | 垂直模型 | 出图通道选「垂直模型」时校验（工单码 `C-ZXCZMX`） | `status==1` | `ai_268`、`ai_5068` |
| `I11` | 爆款生图 | 爆款生图功能（工单码 `C-ZXBKST`） | `status==1` | `ai_5068` `sendChat`/`handleSubmit` |
| `I12` | 需求理解与设计 | Agent `agent_id=2`（需求理解）发消息前校验（工单码 `C-ZXXQLJYSJ`） | `status==1` | `ai_4159`、`ai_7323`、`ai_7987` |
| `I19` | 知识库存储配额 | 知识库容量，`quota = I19.remain × 1024³` 字节 | 直接读 `remain` | `ai_8624` `loadStorageQuotaSize()` |
| `A1` | 展会报告引用 | 「AI 自动匹配展会报告」引用权限 | `status==1` | `ai_3019` `getAuthData` |
| `A2` | 趋势主题引用 | 「AI 自动匹配趋势主题」引用权限（工单提示「无权限引用趋势主题」） | `status==1` | `ai_3019` `getAuthData` |
| `S2` | 成员席位 | 企业空间「成员管理」，`remain` = 可分配席位；`memberTotal - 已用 = 剩余` | 直接读 `remain` | `ai_8718` `memberManage` |
| `S1` | 云存储容量 | 门户「容量管理」，`value` + `unit`（G） | 直接读 | `i_app.js` `capacity` getter |
| `D2` | 版权登记件数 | 版权登记，`authValue` 为总件数，用完弹「版权登记件数用完提醒」 | 直接读 `authValue` | `i_app.js` `getAuth("D","D2","authValue")` |
| `T2 / T26 / T27 / T28` | 图案会员套餐 | 9800 / 15800 两档会员，`T26=锁图先推后买`、`T27=先推后买` | 套餐对比表 | `nuxt_8c391e9.js` |
| `T9 / T13 / T14 / T20 / T25 / B1` | 会员权益对比项 | 会员页权益对比表格展示 | 直接读 | `nuxt_8c391e9.js` |
| `G1` | 设计师权限 | 设计师信息菜单显示 | `cateCode=='G'` 组 | `i_app.js` |
| `G3` | 独立站/选品权限 | `Options` 开关 | `cateCode=='G'` 组 | `i_app.js` |
| `G4` | 店铺权限 | `Stores` 开关 | `cateCode=='G'` 组 | `i_app.js` |

权益码按 `cateCode` 分组返回：`T`=会员套餐、`A`=趋势/展会、`I`=AI 功能、`S`=容量/席位、`G`=设计师与店铺、`D`=版权。调用 `GetAuthList` 时返回的每一项含 `authCode`、`authTitle`、`status`、`remain`、`authValue`、`unit`、`endTime`。

#### 各功能的扣点规则（实测自点数管理页）

| 场景 | 点数 | 备注 |
| --- | --- | --- |
| 分身一 / 分身五 / 分身六 | 2 点 | 按张 |
| 分身二 | 4 点 | 按张 |
| 分身三 / 分身四 | 3 点 | 按张 |
| 垂直模型 / 私有模型 | 1 点 × 图片数量 | |
| 原助理推理 | 1 点/次 | ✅ 点数明细实测 |
| 工具集：更换背景 / 多图背景 / AI 试衣 / 模特场景图 / 三视图 / 图转视频 / 画单线图 / 多图融合 / 系列延伸 / 配辅生款 / 图案应用 / 格式转换 | 5 点 | |
| 工具集：提取线稿 / 无损放大 / 背景移除 / 提取图案 / 画质增强 / 线稿渲染 / 智能扩图 / 自由仿款 | 2 点 | |
| 工具集：涂抹编辑 / 融合创款 | 3 点 | |
| 工具集：转 3D 模型 | 30 点 | |
| 工具集：出血线工具 | 1 点/次导出 | |
| 工具集：自定义改款 | 5 点 | |
| 批量中心：形状填充 | 每个「形状×图案」组合 5 点 | `shape_fill:5`、`series_extend_default:5` |
| 批量中心：消除水印 / 画质增强 / 无损放大 / 背景移除 | 每张 2 点 | |
| 批量中心：系列延伸 | 按模型计费（同工具集） | |
| 批量中心：提取图案 | 每张 3 点起 | |
| 批量中心：替换场景 | 每张产品图 3 点 | |
| 批量中心：批量生图 | 按所选私有模型计费 | |
| A+ 详情图编辑 | 单图编辑 12 点（`toolType:12`） | |

### L3 · 需企业空间 / 管理权限（`orgFlag`、`superFlag`、菜单 `capacity` 控制）

| 接口 | 请求类型 | 前置条件 |
| --- | --- | --- |
| `/user-service/employee/addOrUpdate`、`updateStatus`、`makeOver`、`generateLink`、`get/{id}` | POST/GET | 企业认证（`orgFlag>=2`）+ 成员管理席位 |
| `/user-service/dept/addOrUpdate`、`transfer`、`updateStatus`、`del/{id}` | POST/GET | 同上 |
| `/user-service/role/addOrUpdate`、`transfer`、`updateStatus` | POST | 同上，且 `superFlag` |
| `/user-service/user/createCompany` | POST | 创建企业空间 |
| `/user-service/user/createPersonSpace` | POST | 创建个人空间 |
| `/user-service/merchant/updateName`、`updateLogo` | POST | 机构管理员 |
| `/user-service/employeePoints/updateTotalPoints` | POST | 调整成员额度，管理员 |
| `/user-service/employeePoints/platUsedPoints` | POST | 平台已用点数 |
| `/account-admin/revise/switchWorkspace` | POST | 多主体切换 |
| `/factoryOrder/*`（14 个） | POST | 工厂订单，需企业身份 |
| `/orderLogistic/*`（6 个） | POST | 物流单 |
| `/patternOrder/orderList`、`getSourceFile` | POST | 图案订单 |
| `/designer/settled/*` | POST | 设计师入驻 |
| `/Copyright/*`、`/UserCopyrightRegister/*` | POST | 版权登记，另需 `D2` 件数 |
| `/Finance/*` | POST | 提现、余额，需实名 |

判定入口：`user-service/user/getMyInfo` 返回的 `menuList[].meta.capacity` 数组就是当前账号在这个菜单下允许的操作列表（`view / add / edit / detail / down / apply_paper / cer_upload / cate_edit ...`），前端按它决定按钮显隐。
