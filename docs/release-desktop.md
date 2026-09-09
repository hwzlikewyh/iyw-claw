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

1. 校验版本和 Fusion 登录，拒绝覆盖同版本已有的非 1% 发布策略。
2. 调用 GitHub `Release` 工作流，前端只构建一次。
3. Windows x64/x86 在 GitHub 托管机器并行编译；macOS 两架构与 Linux 同时构建。
4. Windows 签名前输入通过临时草稿资产传输，校验归档 SHA-256 后解包，再核验
   版本、源码提交、架构、文件清单和逐文件摘要。本机只签名与封装。
5. 签名后的资源作为安装验证输入；Windows 实际安装和 macOS Intel 验证成功后，
   清理临时资产，生成更新清单并发布 GitHub 正式版。
6. 本地脚本下载五个平台包和原始更新签名，核对 GitHub 摘要，再上传 Fusion。
7. 所有制品经 Fusion 验签成为 `ready` 后，正式发布并回读更新接口。

安装包和最终记录保留在 `artifacts/release-<版本>/`，这个目录不进入 Git。
脚本运行期间需要保持终端打开；关闭终端不会取消远程构建，但 Fusion 发布会等下次
用同一版本重跑继续。

## 1% 灰度

固定使用 `stable / optional / manual / 1%`，不设置强制更新阈值，不自动逐日扩大。
发布前后均核对 `rolloutBasisPoints=100`，不会把已有 100% 版本改成 1%。

灰度遵循 Fusion 现有规则：自动更新按安装 ID 分桶，手动检查更新可见性仍由服务端
现有逻辑决定。这次没有修改服务端选版逻辑。

## 重试

- 远程构建仍运行时直接跟进，不重复触发。
- 最近同版本工作流失败时，重跑失败 job；成功的编译 job 产物保留供签名重试。
- 如果修改了工作流代码，需要按仓库正式发布重试规则从新 workflow 提交重新派发，
  `gh run rerun` 本身仍使用原工作流代码。
- GitHub 已正式发布时，跳过编译；Fusion 已 `ready` 的相同制品跳过上传。
- 上传过程中断时先尝试完成已有对象校验；对象缺失或长度错误后才重新上传。
- 任一签名、摘要、版本、发布策略或安装验证失败都会停止，不跳过验证强行发布。
