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

staging artifact 包含：

- `out/` 前端静态输出；
- `src-tauri/binaries/` 已校验的 sidecar；
- `src-tauri/resources/runtime-seed/` 及生成的 runtime overlay；
- `src-tauri/target/x86_64-pc-windows-msvc/release/iyw-claw.exe` 已编译但未签名的应用输入；
- `staging-manifest.json`，记录 schema、版本、source commit、target、installer 路径和
  文件 SHA-256。

finalize 脚本在任何签名动作前校验 manifest、版本、target、文件存在性和哈希；不匹配
时立即失败。

## Signing Order

finalize 先执行一次临时文件的 SafeNet 预检。预检每次签名最多等待 90 秒，不读取、
传递或代填 PIN；它只使用 runner 交互桌面中已经建立的 SafeNet 登录会话。会话不存在
时预检失败，不进入任何长耗时步骤。
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
