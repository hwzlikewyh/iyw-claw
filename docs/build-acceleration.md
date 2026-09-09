# 构建缓存与耗时

## 引擎成品缓存

`prepare-xinghe-worker.mjs` 将编译和资源组装分开。各 CI 入口统一使用
`.github/actions/prepare-worker`，本地原有准备命令也支持复用成品。

- 成品按 target 保存在 `harness/xinghe-worker/target/bundle-cache/`，包含引擎、
  辅助程序和 SHA-256 清单。这个目录不会被签名步骤修改。
- 缓存键包含 worker、harness、patches、vendor 源码、Cargo 清单和锁文件、仓库
  Cargo 配置、编译入口脚本、Rust 版本和 host、target、编译环境参数。
- 应用版本、前端、打包脚本和 VC 运行库组装脚本不进入引擎编译缓存键。
- 命中后仍检查文件摘要、架构、上游身份和 ABI 导出，再复制到资源目录。
  缺少或损坏缓存文件会明确失败，不继续打包。
- 缓存核验成功后重建生成资源目录，避免签名机残留上一架构的 DLL。
- VC 运行库每次按当前编译机的正式 redist 目录补齐，最后执行原有完整资源验证。
  Windows 安装验证、代码签名和更新签名流程不因缓存跳过。

从旧资源缓存迁移时需一次重建。不同 target、工具链或编译参数不能共享二进制。
Windows 专用源码变化目前仍保守地影响所有 target，尚未对 Rust cfg 依赖做裁剪。
自定义系统 SDK、原生编译器或全局 Cargo 配置变化后，应清空对应本地成品缓存；
CI 镜像版本和显式编译参数已纳入缓存键。
不含 Git 元数据的源码归档继续使用正常 Cargo 构建，跳过成品缓存。
工作流选择旧版本源码且没有新 action 时，沿用旧准备命令，不要求修改已发布标签。

## 前端与下载

前端复用 `.next/cache` 编译缓存，每次仍从当前源码执行 `pnpm build`。
不把旧 `out/` 当成本次构建输出；同一源码的重试使用同一缓存键。

三个主要入口共用 `runtime-downloads` 缓存命名。下载配置更新时可以复用旧下载，
但所有实际组件仍按当前配置的校验和验证。已有压缩安装包上传不再二次压缩。

## 诊断

worker 编译和主要桌面 CI 构建都启用 Cargo `--timings`。CI 保留
`cargo-timings-<job>-<target>-<attempt>` artifact 七天；其中包含应用和 worker 的
HTML 报告。完整命中引擎缓存时，不会生成本次 worker 编译报告。
日志会显示 `compiler cache hit/miss` 和资源准备耗时。

这批改动不调整 LTO、优化等级、panic、库类型、系统权限或 runner 规格。
不保证全量冷构建十分钟完成；应分别比较首次构建、缓存命中、应用改动和引擎改动。

## 第二批：正常发布分阶段

正常 `Release` 工作流已接入 Windows 托管编译，x64/x86 并行，复用一次前端产物。
签名机只下载已校验输入、签名和封装，失败时可重试失败 job 而不重编成功部分。
签名输入通过 GitHub 草稿资产传输，使用实际验证过较快的 Release 下载通道；
签名成功后的资源才交给独立安装校验，临时草稿资产在正式发布前清理。

手动版本输入、自动上传 Fusion 和固定 1% 灰度的使用方法见 [自动发布](release-desktop.md)。
