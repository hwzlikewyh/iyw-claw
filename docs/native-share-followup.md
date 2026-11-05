# 聊天分享与右侧工作区修复

任务：`01a1aafe-e05b-71f0-96ea-ba719029beec`，2026-11-05。

## 范围与验收

1. 对照 WeChatBridge 源码核查微信、企业微信、飞书、钉钉的桌面入口；支持的入口接入现有导入链路，不能将系统注册成功等同于宿主菜单或端到端可用。
2. 每个转发批次创建独立的新会话草稿，保留正文和附件，持久保存后才确认导入；失败保留批次且可重试，不自动发送。
3. 新对话右侧默认关闭，标题栏右侧按钮打开浏览器，浏览器的加号打开总览／文件／成果面板。总览不重复展示成果列表。
4. 移除账户旁的聊天导入图标，成果菜单复用现有资源区动作。
5. 在最新远程主线验证后提交并推送两个远程的 main。

## 执行顺序

- 完成：源码与发布入口核查 → 上游源码、官方文档、本机已安装制品、远程分支比较。
- 完成：原生注册与新会话交付修复 → Windows 主应用 Rust 检查、接收器 release 构建和本机身份核验。
- 完成：右侧栏代码修复 → TypeScript、定向 lint 和静态导出构建。
- 进行中：制品与跨平台收尾 → 分享包真实签名、资源一致性校验、macOS 编译验证。
- 待完成：合并交付 → 复核远程 main，检查最终 diff 与远程提交号。

遵守仓库 AGENTS.md，不新增或运行测试套件。已有草稿和其他任务工作区不纳入此次提交。

## 已确认事实

- GitHub main `01864533` 比内网 main `08afe57a` 多一个 v0.1.325 版本提交。复用原任务工作树并快进到 GitHub main。
- 本机已安装应用 `D:\software\原助理\app` 的 `native-share` 只有 `.gitkeep`；当前用户没有 `IywAssistant.ShareTarget` 注册。正式发布中缺失接收程序及身份包，不能靠修改分享菜单名称解决。
- Windows staged finalizer 仅恢复环境 helper、worker 和主程序，没有准备或恢复分享组件。
- 上游固定提交为 `3d181a2cc29a9e0b2534f2c36df4ea8d918c0d32`。Windows 当前清单使用 SupportsAnyFileType、StorageItems/Text/URI/Bitmap，与原助理声明相同。上游较早的 `.zip`、可见 AppListEntry、中性名称实验均没有可靠解决所有机器的菜单显示，不照搬失效实验。
- 上游接收器在注册后验证进程的包名和 AUMID；当前实现只检查存在任意包身份，且安装脚本注册后没有再验证。
- macOS 上游通过 AppKit Share Extension 接收用户主动分享的 ZIP，再本地持久化；不读取聊天数据库。

## 平台能力与依据

| 平台 | 桌面入口 | 必要条件与边界 |
| --- | --- | --- |
| 微信 macOS | 系统 Share Extension | 上游要求微信 4.1.13+、macOS 14+；扩展必须随 app 发布并启用。 |
| 微信 Windows | WinRT Share Target | 签名 MSIX、外部资源索引、匹配的 EXE 身份；菜单和实际 ZIP 接收仍须宿主验证。 |
| 企业微信 | WorkBuddy 专用转发入口 | 上游 macOS 实测 5.0.11 不枚举系统 Share Extension；企业微信官方帮助也将 Win/Mac 5.0.11+ 的该入口列为 WorkBuddy 内测能力，未提供任意第三方本地注册协议。 |
| 飞书 | 消息快捷操作 | 官方支持桌面网页应用侧栏及小程序；需要开放平台应用、AppLink 配置、企业可用范围和发布。一次最多 20 条，非本地系统分享注册。 |
| 钉钉 | 酷应用消息菜单 | 官方提供桌面访问地址和 getData；PC 6.5.30+，需要平台应用发布和 JSAPI 鉴权。第三方企业应用不能获取钉盘文件及大尺寸图片／视频。 |

来源：

- [WeChatBridge 企业微信调查](https://github.com/freestylefly/WeChatBridge/blob/3d181a2cc29a9e0b2534f2c36df4ea8d918c0d32/docs/exploration/wecom-forward.zh-CN.md)
- [WeChatBridge Windows 打包与验证记录](https://github.com/freestylefly/WeChatBridge/blob/3d181a2cc29a9e0b2534f2c36df4ea8d918c0d32/docs/design/windows-share-packaging-fix.md)
- [飞书消息快捷操作](https://open.feishu.cn/document/client-docs/extensions/message-shortcut)
- [钉钉消息菜单接入](https://open.dingtalk.com/document/isvapp/how-do-i-access-the-message-menu.md)
- [钉钉获取消息内容](https://open.dingtalk.com/document/isvapp/message-menu.md)
- [企业微信官方转发说明](https://open.work.weixin.qq.com/help2/pc/14758)
- [微软 MSIX 编程签名与时间戳](https://learn.microsoft.com/en-us/windows/win32/appxpkg/how-to-programmatically-sign-a-package)

飞书／钉钉的应用身份与发布配置已向用户询问；不申请全量聊天读取权限、不冒用 WorkBuddy 身份，也不把云端应用注册描述为本地安装即生效。

### 飞书和钉钉接入前置条件

飞书的网页消息快捷操作需要在开放平台启用网页能力，并配置
`https://applink.feishu.cn/client/web_app/open?appId=<应用ID>&mode=sidebar`。
网页在完成 H5 SDK 签名鉴权后，从 URL 的 `bdp_launch_query` JSON 中读取
`__trigger_id__`，交给 `tt.getBlockActionSourceDetail` 获取用户选中的消息。
签名必须包含页面完整 query。此场景不能使用 `tt.getTriggerContext`。
接口最多返回 20 条消息，必须分别处理 `support` 和 `status`，不能把不支持的消息
假装解析成功。依据：[消息详情接口](https://open.feishu.cn/document/getBlockActionSourceDetail)。

钉钉需要启用“酷应用扩展 → 扩展到消息菜单”，配置桌面端 HTTPS 承接地址，并发布应用。
承接页完成 JSAPI 鉴权后，通过 `dingtalk-jsapi/api/util/openTemporary/getData`
取得选中消息及会话信息。PC 要求 6.5.30+；文本、富文本、图片、视频、文件的表示不同，
不能将所有内容当作纯文本。第三方企业应用对钉盘文件和大尺寸媒体的限制需保留，
不能承诺与微信 ZIP 完全相同的附件能力。

这两种平台入口尚未实现和注册。还需确定承接页部署域名、相应应用的网页能力与发布权限，
以及网页消息交给本机原生应用的交接方式；已有机器人渠道的连接凭证不足以证明这些条件
已经具备。当前原生分享组件只处理来源应用实际交给系统的文件。

## 2026-11-06 验证记录

- 本机签名接收 EXE 和 MSIX 均通过签名验证；MSIX 与 EXE 的包名、应用 ID、发布者一致，PRI／图标内外哈希一致。
- 本机 `IywAssistant.ShareTarget` 已注册为 `0.1.325.0`，状态 `Ok`；真实安装路径的接收程序 `--registration-check` 退出码为 0。
- 实际构建发现并修复：PowerShell 7 模块路径污染 Windows PowerShell；中文发布者被 RC 内联清单编码破坏；MSIX 不支持先签名后单独 timestamp；GUI 接收器必须等待进程退出再读取状态。
- TypeScript 与定向 ESLint 通过；webpack 生产构建导出 40 页。构建中 PDF 查看器依赖输出服务端 `window` 引用提示，已定位到既有 `use-pdf-viewer.ts` 静态导入的 `pdfjs-dist/legacy/web/pdf_viewer.mjs`，本次未修改该路径，导出仍完成。
- Windows `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` 通过，输出既有警告；接收器已执行真实 release 构建。
- 本机已有一个飞书机器人渠道配置，但渠道连接不等于网页消息快捷操作已经注册；没有钉钉渠道配置。仍需平台侧的网页能力、可信域名、消息菜单配置和发布权限。
- 新增 PIN 签名扩展已分别在 PowerShell 7 和 Windows PowerShell 5 编译；无 PIN 的可选签名探测已停止，未将其列为通过。当前已执行的实签采用可用的 Windows 签名身份与 signtool；PIN 认证入口的完整运行仍需带现有 CI 签名凭证的构建。
- 跨平台编译使用 [运行 42065927787](https://github.com/hwzlikewyh/iyw-claw/actions/runs/42065927787)，原生源码提交 `4bd1f7e8`。后续 `304cc348` 仅修改前端，`src-tauri` 与该编译源码无差异，最终前端另外完成了本地生产构建和类型检查。
