# 微信聊天记录原生导入

原助理接收用户从微信主动分享的聊天记录。微信中多选消息、合并转发、选择“原助理”，
接收完成后在微信聊天记录窗口选择已打开的本机会话，点击“加入草稿”。完整正文、
原始文件和附件索引会追加到草稿，用户确认后再发送给模型。

侧边栏账户旁的聊天图标可以重新打开导入窗口，显示待处理批次数。
没有系统分享入口时，使用“导入文件”选择微信已导出的 ZIP 或 TXT。
此能力不读取微信数据库，不监听聊天，也不自动发送微信消息。

## 平台与构建

| 平台 | 实现 | 范围 |
| --- | --- | --- |
| Windows | 主程序内置 Rust/WinRT Share Target + 带外部位置的 MSIX 身份包 | Windows 10 2004（19041）及更新版本 |
| macOS | 内嵌 AppKit Share Extension + Rust 主应用 | 扩展目标 macOS 14+，Intel 和 Apple Silicon 分别构建 |
| 旧 Windows / Linux | 本地手动 ZIP/TXT 导入 | 不注册系统分享目标 |

共用代码位于 `src-tauri/share-core`；Windows 接收逻辑位于
`src-tauri/src/native_share_receiver.rs`，直接编译进入 `iyw-claw.exe`，不构建或交付
独立的 `iyw-share-target.exe`。分享激活在 Tauri 单实例检查前处理，文件落盘后再唤起
正常桌面实例；Windows 必需的身份包仍由原助理安装、更新和卸载管理。
主程序使用 `IywAssistant.Desktop` 身份，避免旧 `IywAssistant.ShareTarget` 包的路径或
应用 ID 阻止新 EXE 的首次启动；新包登记验证后移除属于当前安装的旧分享包。
macOS 扩展位于 `src-tauri/native-share/macos`，仅完成系统文件表示接收和持久交接。
Tauri 主应用通过 `commands/native_share.rs` 操作本地收件箱，使用本机 transport，
不会把路径发送到远程服务器。首版只向本机会话交付；远程工作区显示明确提示。

正常桌面构建入口在 `prepare-sidecars.mjs` 中准备系统分享组件。可单独执行：

```powershell
node src-tauri/scripts/prepare-native-share.mjs --target x86_64-pc-windows-msvc
```

Windows 复用既有 `IYW_CLAW_SIGN_*` 签名设置，从现有证书获取 MSIX Publisher。
Azure 等无法自动取得证书 Subject 的方式，使用 `IYW_SHARE_PUBLISHER`，其值必须
与真实签名证书完全一致。构建需要 Windows SDK 的 `makepri.exe`、`makeappx.exe`
和 `signtool.exe`、`mt.exe`。签名器在主 EXE 签名前使用 `mt.exe` 写入 MSIX 身份，
保留原清单的普通用户权限和依赖；随后执行 Authenticode 签名，再生成安装器与更新签名。
不修改已发布安装包的签名字节，不需要 .NET 接收器或编译机的私钥。

Windows 显示资源随正式包完整交付：44/150 基础图标、100%～400% 缩放图标、
16/24/32/48/256 像素的目标图标及 unplated 变体，以及主 PRI 与所有缩放 PRI。
构建和安装包校验使用同一资源规格，检查图像尺寸及包内外资源哈希，缺件时停止打包。

无签名构建保留主程序内置接收代码，不发布可注册的 MSIX；不会创建自签名证书或修改信任库。
手动导入仍可用。`--no-sign` 构建会明确切换为这种行为，避免打包旧的签名制品。

macOS 复用 `APPLE_SIGNING_IDENTITY`、`APPLE_TEAM_ID`。正式签名采用
`<TEAM_ID>.app.iywclaw.share` App Group，主应用和扩展共用同一标识。
扩展先签名，再通过 Tauri `bundle.macOS.files` 放入主应用 `Contents/PlugIns`，
随主应用签名、公证。无签名本地构建使用受限共享目录。
本次不提高主应用现有 13.5 最低系统版本；13.5 不承诺该扩展可用。

安装 Windows 正式包后，安装器尝试为当前用户注册分享入口；注册失败保留正常安装，
应用启动还会检查当前用户的身份、包版本、安装位置和必需资源，缺失或版本落后时自动补注册。
正常安装与更新不要求用户执行配置脚本。健康注册仅查询状态，不重复注册。
状态查询与注册都使用系统 Windows PowerShell 5.1，隔离 PowerShell 7 的模块路径，分别限制在
15 秒和 60 秒内，失败原因进入应用和安装日志。注册先核对包位置和主程序清单，
避免在旧包迁移前启动身份尚未绑定的新 EXE。旧的子目录绑定在安装更新时迁移到
主安装目录；注册不会强制关闭正在运行的原助理，同版本登记被占用时使用正常安装器修复。
更新使用包版本维护入口，卸载先移除
属于当前安装路径的身份包。用户选择保留数据时保留聊天归档，彻底删除时清理本机
收件箱。macOS 在系统设置的共享扩展中启用原助理；微信菜单可能需要重启后刷新。

## 文件与恢复

Windows 和无签名 macOS：用户本机应用数据目录下的 `app.iywclaw/share-inbox`。
正式 macOS：App Group 容器下的 `share-inbox`。

```text
share-inbox/
  Staging/<uuid>/          接收中的批次，尚未对主应用可见
  Ready/<uuid>/
    manifest.json         schemaVersion=1，UTC 时间，原始相对路径和字节数
    files/                已接收的原始文件
    derived/              解析正文、媒体副本、附件索引和准备状态
    imported.json         确认写入会话草稿后的标记
```

复制完全部文件并同步后，整批原子移动至 Ready；系统成功回执发生在提交之后。
通知仅用于唤醒，启动和手动打开窗口都会重新读取持久批次。应用崩溃不会消费
待处理批次；中断的解析重新生成派生文件，进程间文件锁避免同时重建同一批次。
导入后不移动原文件，保持草稿中的路径稳定，也不自动删除仍可能被会话引用的记录。

草稿导入使用一组内联引用一次性追加，保留原有内容。localStorage 写入成功后才
回写已导入标记；写入失败恢复原内容。同一批次引用已在草稿中时不重复追加。
“已加入草稿”不表示已发送给 Agent。发送给云端模型仍遵循原助理现有模型链路。

单次最多 32 个文件，原始文件及整批各限制 512 MiB；整批归档最多 1000 个条目，
总展开量最多 1 GiB，正文合计最多 16 MiB。ZIP 路径、链接、压缩格式、CRC、字节数
均在处理链路中检查。媒体按编号隔离到派生目录，不使用 ZIP 内部路径写出文件。
未知 TXT 格式保留原文并给出警告，不伪造发送人或条数。预览最多显示 40000 个字符。

## 验证边界

本次按仓库约定不新增或运行测试。编译、类型检查、lint、打包语法及静态调用链检查
结果记录在交付说明。macOS SDK 编译、Developer ID/App Group 权限、签名安装包、
真实微信菜单、冷启动、连续分享、升级卸载和两种 macOS CPU 架构均需对应实机验收。

实现参考 WeChatBridge 提交 `3d181a2cc29a9e0b2534f2c36df4ea8d918c0d32`，
MIT 许可原文保存在 `native-share/UPSTREAM-LICENSE` 并随组件发布。
