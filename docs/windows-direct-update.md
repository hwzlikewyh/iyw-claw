# Windows 直接安装与更新

2026-11-16：下载公开应用制品不再要求先登录；已有登录令牌仍按原流程使用。
静默安装继续只读取已校验缓存，避免重走下载时将 ReadyToInstall 状态改为 Verifying，
导致无法领取自动安装任务。此轮对客户端执行 Rust 语法解析及静态调用链检查，未重编主应用。
新版独立向导在父应用退出前准备载荷、WebView2 和环境，成功后自动启动应用。

Windows 10/11 x64 发布同时保留签名 NSIS、`app_payload_zip` 和 `bootstrapper`。
旧客户端的更新字段仍指向 NSIS；新客户端只接受同一 Fusion release 的载荷和向导。
历史 release 没有载荷时继续使用旧更新路径；存在但格式或身份错误时停止更新。
macOS、Linux、Win7 和 i686 不选择直接载荷。

载荷由实际签名后的主程序、环境助手、Tauri resources 映射及微软 WebView2 bootstrapper
组成。独立安装向导只作为 `bootstrapper` 制品下载，不嵌入 NSIS 或 app payload。
确定性 ZIP 清单固定版本、源码提交、架构、每个文件大小和摘要。主应用和助手沿用原
Authenticode 签名，ZIP/向导使用现有 Tauri Minisign 发布密钥。

客户端复用现有 Tauri 下载与更新缓存，额外校验向导大小、摘要、Minisign、Windows
信任链及发布者，并保持禁止写入/删除的文件句柄。向导先验证缓存并预下载缺失环境，
再通知应用正常退出；静默更新在交接前后检查任务、窗口、偏好和更新通道。
交接失败保留应用运行，更新状态明确记录错误。

环境助手 `--identity` 新增 `installerProtocol: 2`。
`install --phase prepare --initialize-defaults` 选择后台默认清单，并返回此次事务编号；
`install --phase recover --transaction-id <id>` 在环境锁内恢复中断激活。
激活意图在目录变更前持久化；事务回执 v2 仅在 current snapshot 写入后标记 committed。
原 NSIS 的 prepare/commit 命令继续兼容。

发布步骤：Fusion 兼容接口部署 → 固定源提交的安装器 0.2.2 草稿签名与验收 →
安装器稳定版 → 含三种制品的应用过渡版。签名任务只补打包已有应用构建结果，
保留原 NSIS 供旧客户端首次迁移。新程序从此后使用向导直接替换 app 文件。
公开下载页按 Fusion latest 返回的 Windows URL 选择制品，无需固定 NSIS 文件名。

本地验证：桌面 cargo check --lib、实际历史发布签名/篡改拒绝、原生 Windows
发布者信任链、环境助手隔离恢复、确定性 ZIP 与完整资源映射均通过。
CI 新增签名载荷直接安装、旧安装迁移、重复更新和缺失 uv 补齐检查。
这些 CI 验收尚未运行，UAC 和完整卸载也尚未实测；本地检查不代表已正式发布。
