# Staged Windows Signing Test Design

## Goal

验证 Windows x64 桌面包可以先在 GitHub hosted runner 编译，再由带 SafeNet
硬件证书的 self-hosted runner 下载、签名并上传最终 artifact。该流程只用于
测试，不创建 GitHub Release，也不修改正式发布 workflow。

## Workflow Boundary

新增 `.github/workflows/release-staged-windows.yml`，仅支持 `workflow_dispatch`。

- `build-staging` 在 `windows-2022` 上固定构建 Windows x64 unsigned NSIS 输入。
- `finalize-signing` 在 `[self-hosted, Windows, X64, iyw-signing]` 上下载 staging
  artifact，恢复已编译的 target 目录，使用仅供该流程的 SafeNet signer 并上传 final artifact。
- 两个 artifact 使用不同名称：`staging` 只保留两天，`final` 只在签名校验成功后
  上传。流程不调用 `release.yml`、`release-tauri.yml`，不创建或发布 Release。

## Artifact Contract

staging artifact 内部包含：

- `src-tauri/target/x86_64-pc-windows-msvc/release/iyw-claw.exe` 已编译但未签名的应用输入；
- `staging-manifest.json`，记录 schema、版本、source commit、target、installer 路径和
  文件 SHA-256。

finalize 脚本在任何签名动作前校验 manifest、版本、target、文件存在性和哈希；不匹配
时立即失败。

为避免 GitHub artifact 传输可重建输入，staging 不跨 runner 传输 `out/`、sidecar 或
runtime seed。签名 job 在固定 source checkout 后重新执行 `pnpm build`，并使用持久缓存
本地准备、校验 sidecar 和 runtime seed；跨 runner 只传 unsigned 应用二进制及 manifest。
hosted runner 使用 Optimal ZIP 压缩，签名机再下载单个 `iyw-windows-staging.zip` 并解压。
随后仍执行相同的 manifest 校验；ZIP 只是传输封装，不改变二进制的 SHA-256 契约。
专用 SafeNet runner 的外部网络由本机代理提供；finalize job 显式使用该 runner 的
`http://127.0.0.1:7890` 代理，通过 GitHub API 获取 artifact URL，并使用可断点续传、
大小校验和 SHA-256 校验的 `curl` 下载，避免 Azure Blob artifact 下载绕过代理后产生
截断 ZIP。不提供可变代理输入，也不向签名器传递 PIN/password。

## Signing Order

finalize 先执行一次由未签名 `agent-browser` sidecar 派生的临时文件 SafeNet 预检，
避免把已由 Node.js 发布方签名的运行时二次签名。预检及每次 staged 签名最多等待
180 秒，以覆盖 SafeNet 对 NSIS 临时文件已观测到的超过 90 秒耗时；超时后的签名校验
也有独立的 30 秒上限。仍不读取、传递或代填 PIN。它只使用 runner 交互桌面中已经建立
的 SafeNet 登录会话。会话不存在时预检失败，不进入任何长耗时步骤。
通过后生成绝对路径 signing overlay，调用 `tauri bundle --target ... --bundles nsis`
从已编译的应用输入重新生成 NSIS。Tauri 的 `signCommand` 负责主程序、NSIS 组件、
临时卸载器和最终 installer；固定的 `agent-browser` sidecar 保持原字节并跳过
Authenticode 签名。

bundle 完成后运行 `verify-signatures.mjs` 校验最终 installer，只有校验成功才上传
final artifact。该测试关闭 updater artifact 生成，因为它不发布 updater manifest；正式
发布仍由原流程负责生成 `.sig` 和 `latest.json`。

## Failure Handling

- SafeNet 不可用、manifest 不匹配、bundle 失败或签名校验失败都会停止流程；不会上传
  final artifact。
- 不读取、传递或记录 PIN/password。SafeNet 登录会话由 runner 交互桌面维护。
- staging artifact 保留用于诊断，final artifact 只在完整校验后产生。

## Verification

对新增 workflow 执行 Prettier 和 actionlint；对 finalize 脚本执行 `node --check`。
首次运行只传 Windows x64，确认 staging/final 两个阶段的 job 状态、manifest 校验、
Authenticode 校验和 artifact 元数据。
