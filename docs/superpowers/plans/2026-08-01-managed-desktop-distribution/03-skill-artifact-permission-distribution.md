# Task 03：Skill artifact、权限与 TOS 直下

## 目标

修复 `expected_size/received_size`，将逐文件版本异步构建为确定性 ZIP 并存入 TOS；实现官方、同组织、用户私有和强制分发的统一授权；桌面端按安装计划从 TOS/CDN 下载。

## 根因基线

- `direct_upload_init.go` 把原始总大小写入 `PackageSize`。
- `download.go` 运行时 Deflate ZIP，实际响应长度未知。
- 桌面 `install.rs` 用原始总大小验证 ZIP，重试无法修复语义不匹配。
- 逐文件版本没有冻结的 ZIP 对象，`ObjectSHA256` 也不能代表动态生成的响应；只删除 size 校验后仍会在对象摘要校验失败。
- `access.go`/repository market filter 只支持 global public 或 owner private，没有 organization audience。
- 文档仍要求所有下载经 Fusion API 流式代理，必须更新。

## 依赖

- Task 01 schema/contract 已冻结。
- Task 02 提供 job handler registry；若尚未合并，可先实现纯 handler/service，禁止临时创建第二套 goroutine ticker。

## scope_write

后端：

- `internal/domain/skill/`
- `internal/application/skill/`
- `internal/adapter/mysql/skill_*.go`
- `internal/adapter/httpserver/skill*/`
- 新增 Skill 专用 object storage adapter 文件；禁止修改共享 `tos.go`，共享 client 构造由 Task 13 接线
- Skill contract/操作文档和聚焦测试

桌面：

- `src-tauri/src/commands/skill_market/`
- 与 Skill 下载票据和本地安装 inventory 直接相关的 Rust 模块
- `src/lib/skill-market.ts` 中非 UI 的 contract 类型/调用

## 禁止修改

- SQL、后端 router/bootstrap、`docs/admin`、桌面 Skill React UI、Tauri 根配置、系统 Skill Git 模块。

## 后端实现

### 统一 access policy

建立一个领域判定入口，输入 identity、Skill、operation，操作至少含 list/get/files/versions/dependency/install/download/manage。

- global official：任意已登录用户读。
- organization：`identity.OrgCode == skill.OrgCode` 读；同一组织内所有已登录用户可见，不要求创建者。
- owner private：创建者 identity 三元组匹配才读。
- manage：创建者或明确 admin；官方发布规则保留。
- 未登录一律拒绝。
- 依赖遍历每一跳复用同一 policy。

repository 查询先做组织/受众过滤，application 再做对象级判定，防止越权和 TOCTOU。不可见对象统一 not found/denied 语义。

### 上传与构建状态机

1. complete 上传只把 version 推到 `artifact_pending`，不直接 ready。
2. enqueue `skill_artifact_build:{version_id}:{generation}`。
3. handler HEAD 每个对象并校验 path/size/sha/file count/content digest。
4. 确定性 ZIP 写临时文件：排序路径、固定 UTC 1980 时间、固定权限、禁止额外字段。
5. 在生成时同步计算 artifact size/sha；设置压缩前后大小上限。
6. 上传临时 TOS key，HEAD/抽检后登记不可变正式 key。
7. 事务提交 ready artifact + active artifact + version ready。
8. 失败记录 code，Skill 不进入可安装列表；原始文件保留用于重跑。

### 历史回填

- 扫描 `StorageKey == ''` 且逐文件齐全的 ready 版本。
- 分页 enqueue，可暂停、可重跑、可查看进度。
- 已有相同 ready digest 的 generation 跳过。
- 回填期间旧客户端是否继续动态 ZIP 由 feature flag 控制；新客户端绝不使用 legacy size。
- 关闭旧流前，统计过去 30 天 legacy 请求为零。

### 票据

- install plan 返回 artifact metadata 和 ticket endpoint。
- ticket handler 重查 identity、audience、version status、artifact ready、plan version和客户端上下文。
- 返回 TOS/CDN 短时 URL；不返回 object key。
- URL 过期可刷新；plan 过期必须重新 resolve。
- Fusion API 不读取/代理 ready artifact body。

## 桌面实现

- 下载器以 `artifactSize` 校验 Content-Length/最终字节，以 `artifactSha256` 校验文件。
- 支持 `.part`、Range、ETag/If-Range、票据刷新和有界重试。
- 服务端不支持 Range 时清空不可信 part 后完整重下。
- 校验通过再安全解压到 staging；防 Zip Slip、符号链接、设备文件、重复路径、文件数/展开大小炸弹。
- 安装 manifest 记录 artifact ID、sha、version、source audience 和 installedAt。
- 已有相同 ready manifest 直接复用。
- 原子替换 active 指针；失败保持旧版本。
- 市场 Skill、系统 Skill、用户目录分开，不能覆盖用户修改。

## 兼容与 API 文档

- 更新 `docs/skill-market-user-client-integration.md`：废弃实时 ZIP 流示例，改为 plan -> ticket -> TOS。
- 明确 `artifactSize != rawSize`。
- 旧 `/skills/download` 在兼容期返回重定向或受控 legacy 行为，不向新端提供错误 metadata。
- ID 全字符串，依赖版本固定，不下载 latest 替代计划版本。

## 测试矩阵

- 复现 19644 raw -> 14339 ZIP，安装成功且只比较 artifact size。
- 实际 ZIP object SHA 非空并与下载字节一致；不能用 content digest 代替 object digest。
- 0 字节/已压缩文件/Unicode 路径/大量小文件/嵌套目录。
- HEAD 与 GET 不一致、对象缺失、ZIP 生成中断、TOS 上传后 DB 失败。
- 同版本重复 build、双 worker、stale fencing。
- 同组织成员可见；跨组织、匿名、其他用户 private 不可见。
- 列表、详情、文件、版本、依赖、计划、票据的权限结果一致。
- URL 过期、断点续传、错误 Range、摘要错、磁盘满、退出后恢复。

## 验证

- Fusion 运行领域/application/repository/HTTP contract 测试和 `go test ./internal/...` 相关包。
- 桌面本机不运行测试或编译；静态追踪 plan -> ticket -> download -> verify -> extract -> activate -> rollback。
- 在远端 Windows CI 用真实 TOS 测试账号完成安装，确认 Fusion 出口流量不承载 ZIP body。

## 完成定义

- 根因用例修复，历史版本有回填策略。
- 四类权限和依赖闭包无越权。
- 新客户端所有 Skill 大字节直下 TOS/CDN。
- 失败不会破坏现有 Skill 或用户目录。
