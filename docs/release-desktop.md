# 自动构建和 Fusion 灰度发布

在 PowerShell 中运行，按提示输入版本号：

```powershell
.\scripts\release-desktop.ps1
```

也可以直接指定版本，或只做只读检查：

```powershell
.\scripts\release-desktop.ps1 -Version 0.1.195
.\scripts\release-desktop.ps1 -Version 0.1.195 -CheckOnly
```

示例版本号应替换为本次要发布的新版本。第一次使用前，必须先将这次脚本和工作流
改动合并推送到 GitHub `main`；脚本发布的是远程主分支，不会上传本地未提交代码。

## 前置条件

- Node.js 22 或以上，GitHub CLI 已通过 `gh auth login` 登录且有工作流和发布权限。
- 本机环境变量 `IYW_FUSION_ADMIN_TOKEN` 为有效的 Fusion 管理凭证。
- 原助理已登录；脚本优先读取 `.iyw-claw/iyw-account-token.json` 中的网关会话。
  也支持环境变量 `IYW_FUSION_GATEWAY_TOKEN`。
- Windows 签名 runner 在线、SafeNet 会话有效；仓库已有 `WINDOWS_SIGN_THUMBPRINT`
  变量和 Tauri 更新签名 Secrets。私钥只在签名 job 中使用，不下载到本地脚本。
- 本机下载使用 GitHub CLI 的现有代理环境；必要时先设置 `HTTPS_PROXY`。

脚本不会将凭证写入源码或发布记录。登录失效时会停止，重新登录后用相同版本号重跑。
`-CheckOnly` 只检查版本、权限、Fusion 登录和远程新流程是否存在，不创建发布。

## 自动流程

2026-11-19 起，后续发布关闭 Win7 双架构自动打包，仓库变量
`IYW_WIN7_RELEASE_ENABLED=false`。默认构建 Windows 10/11 x64、macOS 双架构和
Linux 双架构；缺少 Win7 制品不阻塞 Fusion 同步。已发布的 Win7 安装包仍保留。
已启动的旧工作流使用启动时的任务图，不为关闭 Win7 而重跑其他平台。

1. 校验版本和 Fusion 登录，拒绝覆盖同版本已有的非 1% 发布策略。
2. 调用 GitHub `Release` 工作流，前端只构建一次；独立 worker job 同时启动，
   优先复用引擎源码变更后预构建的成品。
3. Windows x64 在 GitHub 托管机器编译；macOS 两架构与 Linux 同时构建。
   可选 Linux ARM64 使用原生 ARM runner 提前构建，不阻断四个必选平台发布；Intel macOS
   安装验证在自身构建完成后启动，不等待其他平台。编译提速参数和权衡见
   [构建缓存与耗时](build-acceleration.md)。
   各端应用编译与 worker 编译在不同 runner 并行，组装前按源码和架构接收并校验
   worker；macOS/Linux 使用 `tauri bundle` 组装，不再重复编译应用。
4. Windows 签名前输入通过临时草稿资产传输，校验归档 SHA-256 后解包，再核验
   版本、源码提交、架构、文件清单和逐文件摘要。本机只签名与封装。
5. 签名后的资源作为安装验证输入；Windows 实际安装和 macOS Intel 验证成功后，
   清理临时资产，生成更新清单并发布 GitHub 正式版。
6. 本地脚本核对 GitHub 正式版的摘要，并将各平台包直传 TOS。受保护的验签作业从 TOS 读取指定版本，校验实际字节和更新签名，输出签名回执。
7. 将回执按 `<制品ID>.json` 放入 `FUSION_RECEIPT_DIR` 指定的目录，再用相同版本运行本地脚本。发布脚本只向 Fusion 提交回执，所有已登记制品进入 `ready` 后才发布并核对更新接口。GitHub 的 `Publish Fusion Release` 工作流也能复用已由验签作业提交的回执。提示 `APP_RELEASE_MATRIX_INCOMPLETE` 时，可在草稿详情查看仍在等待验签的平台包。

安装包和最终记录保留在 `artifacts/release-<版本>/`，这个目录不进入 Git。
脚本运行期间需要保持终端打开；关闭终端不会取消远程构建，但 Fusion 发布会等下次
用同一版本重跑继续。

## 1% 灰度

固定使用 `stable / optional / manual / 1%`，不设置强制更新阈值，不自动逐日扩大。
发布前后均核对 `rolloutBasisPoints=100`，不会把已有 100% 版本改成 1%。

灰度遵循 Fusion 现有规则：自动更新按安装 ID 分桶，手动检查更新可见性仍由服务端
现有逻辑决定。这次没有修改服务端选版逻辑。

## 重试

只有 Windows 封装或签名失败、应用编译已成功时，使用现有草稿与原 Release 运行编号：

```powershell
gh workflow run release-resign-windows.yml -R hwzlikewyh/iyw-claw --ref main -f version=0.1.340 -f build_run_id=43679471534
```

该流程校验原运行的固定源码、staging 摘要、版本和架构，只重新打包、签名和安装验收，
不重新编译应用。正式发布还要等待原 macOS 构建及安装检查成功。若当前构建尚未结束，
流程先等待原 Windows 构建结果；不启动第二轮编译。

签名向导作为单独的 `bootstrapper` 制品下载；随后从 Fusion 获取 app payload。
向导文件不嵌入 NSIS，也不嵌入应用 ZIP。
正常发布与重新签名流程固定使用已发布的 `v0.2.11` 统一安装器，启动时识别已有安装并沿用原位置更新。

Tauri 会在封装 NSIS 前把主 EXE 的 `__TAURI_BUNDLE_TYPE_VAR_UNK` 改成 `NSS`。
必须启用打包器的 `signCommand` 回调，对改写后的 EXE 签名；仅在 bundle 前签名会得到
外层安装器签名有效、内层 EXE `HashMismatch` 的错误包。0.1.339 的已签名源文件与
安装包内 EXE 恰好相差这 3 个字节，本次据此修复；不得通过关闭内层验签绕过。
打包器恢复的原始签名 EXE 继续用于直接载荷 ZIP。

NSIS smoke 卸载使用 `_?=<已验证临时安装目录>` 原地运行，等待真实卸载完成，
之后才检查和清理临时目录。不能把卸载启动器退出当成卸载已经完成。

- 远程构建仍运行时直接跟进，不重复触发。
- 最近同版本工作流失败或被取消时，从远程主分支重新派发完整流程，重新核对草稿和源码，
  各平台统一使用新流程输出的 Release ID；编译缓存仍可复用。
- 草稿已删除、重建或 ID 返回 404 时，不能只重跑失败 job，否则会继续引用旧 ID。
  同版本标签和手动发布共用并发锁；构建期间不要删除、重建草稿或移动标签。
- 如果修改了工作流代码，需要按仓库正式发布重试规则从新 workflow 提交重新派发，
  `gh run rerun` 本身仍使用原工作流代码。
- GitHub 已正式发布时，跳过编译；Fusion 已 `ready` 的相同制品跳过上传。
- 上传过程中断时先尝试完成已有对象校验；对象缺失或长度错误后才重新上传。
- 任一签名、摘要、版本、发布策略或安装验证失败都会停止，不跳过验证强行发布。

发布资产上传每次最多等待五分钟，最多尝试三次；响应超时后先回读对象摘要和长度，
已经上传成功时直接复用，未完成才重试。签名机对 `uploads.github.com` 使用现有代理，
避免大型载荷绕过代理而长时间阻塞。

Linux 与 Win7 某一架构失败时，可分别使用 `Repair Linux Release`、
`Repair Windows 7 Release`，传入原草稿的版本、源码提交、Release ID 与前端 run ID。
原 run 保留失败记录，其它架构继续完成。全部补建成功后，`Verify Final Desktop Release`
的 `repair_run_ids` 接受逗号分隔的补建 run ID，并核对补建工作流、目标、签名结果和
日志中的相同源码/草稿身份。最终发布同时要求 Linux 双架构；开启 Win7 发布时还要求
Win7 双架构编译与签名通过，保留 Windows 实际安装和 macOS Intel 验证。
