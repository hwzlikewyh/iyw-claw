# 自有 Runner 测试打包设计

## 目标

新增独立的 GitHub Actions workflow，验证现有 Windows 和 macOS self-hosted
runner 的桌面打包环境；既有 `release.yml`、`release-tauri.yml` 和
`release-server.yml` 保持不变。

## 范围

- workflow 通过 `workflow_dispatch` 接收版本号和源码引用。
- Windows x64/i686 使用 `self-hosted, Windows, X64`。
- macOS x64/arm64 使用 `self-hosted, macOS, ARM64`。
- 前端构建继续使用 GitHub-hosted Ubuntu，仅作为桌面构建输入。
- self-hosted job 优先复用 Node.js 20+；缺失时由 `actions/setup-node` 自动安装 Node.js 24。
- Rust target 由 `dtolnay/rust-toolchain` 自动安装；Windows/macOS 系统级编译依赖由
  预检步骤检查并给出可执行的安装提示。
- 构建使用 `--no-sign`，结果上传为 Actions artifact；不创建 GitHub Release，
  不调用 Fusion 发布接口，不改变正式更新通道。

## 验收

1. 新 workflow 文件独立存在，旧发布 workflow 的内容不变。
2. Windows 和 macOS job 的日志能显示对应 self-hosted runner，并完成目标平台
   的 Tauri bundle 或明确报告环境缺失。
3. 每个成功目标上传独立的测试 artifact，工作流状态可追踪。
4. 已有 Node.js 20+ 的 runner 不再重复下载 Node；缺少 Node 时进入自动安装路径。
