# Windows 代码签名（Authenticode）

> 目的：消除「未知发布者」与杀毒软件误报。
> 前置结论：**没有免费的、能被 Windows 信任的代码签名证书。** 自签名证书对 SmartScreen
> 和杀软无效（除非在每台目标机器上手动导入根证书），部分杀软还会因「自签名 + 网络下载」
> 组合而提高怀疑度。可选项见文末「证书从哪来」。

## 为什么必须签名

`iyw-claw` 的正常功能在杀软的行为模型里叠加了多项高权重特征：

- NSIS 安装器 `taskkill /F /T` 强杀进程树、`RMDir /r` 递归删目录、写注册表、装到 `%LOCALAPPDATA%`
- `externalBin` 释放 sidecar exe（dropper 特征）
- 运行期从 GitHub / 镜像下载二进制并执行（ACP binary cache、`uv tool install` 目前无校验和）
- `office_tools.rs` 把 `raw.githubusercontent.com` 的安装脚本管道进 shell
- Web 服务可绑 `0.0.0.0` + WebSocket
- 读取凭据存储（keyring / git credential）、起终端子进程
- Rust 静态链接产物体积大、段熵高，易被误判加壳

这些单独看都是正常的。**签名的作用是给它们提供可追溯的责任主体**：同样的行为，签名程序放行，
无签名程序命中启发式。所以签名是前提，不是优化项。

注意：`TAURI_SIGNING_PRIVATE_KEY` 是 **updater 的 minisign 密钥**，用于校验自更新包，
与 Authenticode 是两套完全独立的机制。仓库里早已配置前者，但在本次改动前**从未有过 Authenticode**。

## 本地签名构建

```powershell
# 证书已装进 Windows 证书存储（USB token / 云 HSM 的标准形态）
$env:IYW_CLAW_SIGN_MODE = "signtool"
$env:IYW_CLAW_SIGN_THUMBPRINT = "<证书 SHA-1 指纹>"
pnpm tauri:build:signed

# 构建后校验所有第一方可执行产物都带签名
pnpm sign:verify
```

指纹可以从 `certmgr.msc` 复制（带空格也行，脚本会清理），或：

```powershell
Get-ChildItem Cert:\CurrentUser\My | Select-Object Thumbprint, Subject
```

## 环境变量

| 变量 | 说明 |
| --- | --- |
| `IYW_CLAW_SIGN_MODE` | `signtool` \| `pfx` \| `azure` \| `none`（默认 `none`，即不签名） |
| `IYW_CLAW_SIGN_REQUIRED` | 设为 `1` 时，`mode=none` 直接构建失败 —— 用于发布流水线兜底 |
| `IYW_CLAW_SIGN_THUMBPRINT` | 证书存储中的 SHA-1 指纹（`mode=signtool`） |
| `IYW_CLAW_SIGN_PFX` / `_PFX_PASSWORD` | .pfx 路径与密码（`mode=pfx`） |
| `IYW_CLAW_SIGN_AZURE_DLIB` / `_AZURE_METADATA` | Azure Artifact Signing 的 dlib 与元数据 json（`mode=azure`） |
| `IYW_CLAW_SIGN_TIMESTAMP_URL` | RFC 3161 时间戳服务器，默认 `http://timestamp.digicert.com` |
| `IYW_CLAW_SIGN_DIGEST` | 摘要算法，默认 `sha256` |
| `IYW_CLAW_SIGNTOOL` | 显式指定 `signtool.exe`，跳过 SDK 自动发现 |
| `IYW_CLAW_SIGN_NONINTERACTIVE` | 设为 `1` 时不传递或重试 PIN；token 未预先登录会在 30 秒内失败 |

`mode=pfx` 只适合本地冒烟测试：signtool 通过命令行接收 .pfx 密码，子进程存活期间密码在进程列表里可见。
真实发布用 `signtool` 或 `azure`。

## 签哪些文件、按什么顺序

三个环节，缺一个就等于没签：

1. **第一方 sidecar** —— `prepare-sidecars.mjs` 在 cargo 产出后、拷贝到各处之前签名一次。
   之后 `binaries/` 和 bundle 兼容别名都是这个已签名文件的副本，不存在某个布局漏签。
   这一步必须在这里做:默认构建路径下该脚本跑在 `tauri build` 的 `beforeBuildCommand` 内部，
   外层脚本没有插入点。
2. **主程序与安装器** —— 由 bundler 通过 `bundle.windows.signCommand` 调用 `sign-windows.mjs`。
3. **updater 的 `.sig`** —— Tauri 在 Authenticode 之后基于安装器字节计算 minisign 签名。

顺序不能颠倒：如果在 bundler 之后再补签 Authenticode，会改动安装器字节从而使 `.sig` 失效，
自更新校验就会失败。所以签名必须由 bundler 驱动，不能作为构建后的独立步骤。

`agent-browser` 是例外：它保持上游发布字节不变，由构建脚本和运行时共同校验固定大小与
SHA-256，不加入宽泛的 Authenticode 信任边界，也不计入第一方签名扫描。

## 为什么用 signCommand 而不是 certificateThumbprint

2023 年 6 月起 CA/B Forum 要求 OV 代码签名私钥存放在 FIPS 140-2 Level 2 及以上硬件中
（USB token 或云 HSM），因此**不存在可直接导入的 .pfx**，`certificateThumbprint` 那条路
在多数真实证书上走不通。`signCommand` 是唯一同时支持硬件密钥和 Azure Artifact Signing 的形态。

配置覆盖层由 `prepare-signing-config.mjs` 生成到 `src-tauri/.signing.conf.json`（已 gitignore），
因为 `signCommand` 需要绝对路径 —— bundler 调用命令时的工作目录不属于 Tauri 的公开契约，
用相对路径是碰运气。文件名刻意不叫 `tauri.windows.conf.json`：那个名字会被 CLI 在每次
Windows 构建时自动合并，会导致所有本地构建都去找证书。签名保持 `--authenticode` 显式开启。

设置了 `signCommand` 后，Tauri 会忽略 `digestAlgorithm` 和 `timestampUrl`（它们只用于拼默认
signtool 命令），所以覆盖层里不写这两项，摘要和时间戳统一由环境变量控制,避免两个来源。

## CI

`.github/workflows/release-tauri.yml` 里的 `Configure Windows Authenticode signing`
在 sidecar 暂存**之前**运行，通过 `$GITHUB_ENV` 把配置传给后续所有步骤。

按需配置以下签名来源之一：

- 自托管 Windows runner：仓库变量 `WINDOWS_SIGN_THUMBPRINT`（SafeNet token 中证书的 SHA-1 指纹）
- `WINDOWS_SIGN_PFX_BASE64` + `WINDOWS_SIGN_PFX_PASSWORD`（base64 编码的 .pfx）
- `WINDOWS_SIGN_AZURE_METADATA` + `WINDOWS_SIGN_AZURE_DLIB`（Azure Artifact Signing）
- 可选变量 `WINDOWS_SIGN_TIMESTAMP_URL`

都没配置时，构建照常进行但产物无签名，并在 job summary 里留一条 warning。
`Verify Authenticode signatures` 步骤在未配置时是 advisory（`--warn`），配置后转为强制失败。

USB token 无法在 GitHub 托管 runner 上使用（需要物理设备）。Windows release 矩阵通过
`iyw-signing` 标签固定到装有 SafeNet 客户端并插入 token 的 self-hosted runner，使用
`WINDOWS_SIGN_THUMBPRINT` 配合 `mode=signtool`；也可以改用云 HSM / Azure。

自托管测试 workflow 默认启用 `IYW_CLAW_SIGN_NONINTERACTIVE=1`。请在启动 job 前
通过 SafeNet Authentication Client 的 Single Logon 让当前交互会话完成一次 token
登录；workflow 不保存或传递 PIN。若 token 未登录，签名步骤会超时失败并停止上传，
不会在 workflow 中输入或重试密码；SafeNet 客户端自身是否显示提示由其策略决定。

## 校验产物

```powershell
pnpm sign:verify              # 任一第一方产物无签名则 exit 1
pnpm sign:verify -- --warn    # 只报告
node src-tauri/scripts/verify-signatures.mjs path\to\one.exe   # 指定文件
```

输出区分三种结论,这个区分很重要 —— 把「链不受信」报成「未签名」会让人查错方向：

- `signed` —— 签名存在且链可信
- `UNTRUSTED` —— **签名存在**，但证书链没有受信根（自签名证书，或验证机器上缺中间证书）
- `UNSIGNED` —— 完全没有签名，说明该产物根本没走签名流程

## 证书从哪来

按可行性排序:

1. **国内 CA 的 OV/EV 证书**（约 ¥1500–8000/年）。对国内发行最现实。EV 直接跳过
   SmartScreen 信誉门槛，OV 需要逐步积累下载量信誉。密钥在 USB token 或云 HSM 上。
2. **SignPath Foundation** —— 面向开源项目**免费**。本仓库是 Apache-2.0，但其条款要求
   项目不含任何专有组件、且构建全自动且公开。`iyw-fusion-api` 属于专有后端，
   需要先确认这是否构成障碍。
3. **Azure Artifact Signing**（前 Trusted Signing）—— 约 $9.99/月，但截至 2026 年 4 月
   仅面向美国/加拿大/欧盟/英国的企业及个体经营者，中国主体不符合资格。
4. **自签名** —— 免费但对本目的无效，仅用于跑通流水线（`mode=pfx`）。

拿到证书并签名之后，仍可能有残留误报，此时再走各家白名单申报：
微软 MSRC 误报提交、360 / 火绒 / 腾讯各自的开发者申诉入口。
**顺序很关键**:签名之前提交申诉不会被加白。

## 仍然存在的减分项

签名不能替代这些，它们仍会被杀软计分：

- ACP binary cache 与 Agent-Reach `uv tool install` **无校验和**（已知取舍，计划随版本中心
  签名票据方案一并解决）。对比参考：`commands/runtime_bootstrap/fallback/spec.rs` 里
  Node/Git/uv 都有 pinned SHA-256，坏代理只会导致失败切换而不会执行被篡改的代码。
- Web 服务默认绑定 `0.0.0.0`(仅在用户主动开启 web 服务时生效，`auto_start` 默认关闭)。
- `src-tauri/binaries/` 里遗留了多个 0 字节的历史版本 exe,属于多余的可疑释放物。
- 文档中推荐 `irm ... | iex` 的安装方式,这是杀软告警优先级最高的模式之一。
