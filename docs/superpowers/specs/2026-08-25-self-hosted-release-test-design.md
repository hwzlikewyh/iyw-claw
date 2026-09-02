# 自有 Runner 测试打包设计

## 目标

新增独立的 GitHub Actions workflow，验证现有 Windows 和 macOS self-hosted
runner 的桌面打包环境；既有 `release.yml`、`release-tauri.yml` 和
`release-server.yml` 保持不变。

## 范围

- workflow 通过 `workflow_dispatch` 接收版本号和源码引用。
- Windows x64/i686 使用 `self-hosted, Windows, X64, iyw-signing`，并强制使用
  runner 本机证书完成 Authenticode 签名。
- 默认只构建 Windows x64；需要完整架构验证时通过 `windows_arches=both`
  增加 i686。
- macOS x64/arm64 保留为 `run_macos=true` 时才运行的可选验证，避免离线 runner
  阻塞 Windows 证书测试。
- 前端构建继续使用 GitHub-hosted Ubuntu，仅作为桌面构建输入。
- self-hosted job 优先复用 Node.js 20+；缺失时直接下载并安装 Node.js 24。
- Node.js 和 Rust target 由平台 shell 直接自举，避免 self-hosted runner 在拉取环境
  action 时受 GitHub codeload 网络波动影响；Windows/macOS 系统级编译依赖由预检
  步骤检查并给出可执行的安装提示。
- Windows runner 复用本机 pnpm store、`node_modules`、Cargo target、sccache 和
  runtime-seed 下载缓存，并在本机生成前端输出，避免慢速 Actions artifact 下载。
- macOS 启用时仍复用 Ubuntu 前置任务生成的 `out/`。
- self-hosted checkout 固定使用 Git HTTP/1.1，规避部分 Mac 网络上的 HTTP/2
  framing 错误。
- workflow 可分别为 Windows/macOS runner 注入可选 HTTP(S) 代理，避免把代理地址
  硬编码进仓库。
- Windows 构建使用仓库锁定的 Tauri CLI 和本地证书，构建后只校验本次新生成的
  installer；任何准备、构建或签名错误都会终止 job，不上传失败产物。
- 结果只上传为 Actions artifact；不创建 GitHub Release，不调用 Fusion 发布接口，
  不改变正式更新通道。

## 验收

1. 新 workflow 文件独立存在，旧发布 workflow 的内容不变。
2. 默认 Windows x64 job 在 `iyw-signing` runner 上完成 Tauri NSIS bundle，且
   `verify-signatures.mjs` 对本次 installer 验证成功。
3. 仅成功目标上传独立的测试 artifact，工作流状态可追踪。
4. 已有 Node.js 20+ 的 runner 不再重复下载 Node；缺少 Node 时进入自动安装路径。
5. 同一测试版本重新触发时取消旧任务，矩阵任一目标失败时停止其余目标。
