# 各平台发布速度修复

用户已授权优化所有平台打包速度，并要求保留现有安装、签名和旧版兼容。

## 测量与验收

基准为 0.1.350 Actions run 43996332197，原始 Cargo timings 和资源记录保存在
`D:/projects/iyw/artifacts/build-speed-0.1.350`。

| 目标 | 编译 | 实际打包 |
| --- | --- | --- |
| Linux x64 | 37.5 分钟 | AppImage 2.5 分钟 |
| Linux ARM64 | 70.3 分钟 | DEB 0.5 分钟；RPM 超过 60 分钟仍未结束 |
| macOS ARM64 | 108.9 分钟 | 2.2 分钟 |
| macOS Intel | 超过 129 分钟 | 仍在编译 |
| Windows x64 | 59.1 分钟 | 签名与两种包 5.7 分钟 |
| Win7 x64/x86 | 61.7 / 77.2 分钟 | 签名打包 5.5 / 5.8 分钟 |

macOS runner 仅 7 GiB RAM，记录超过 400 万次换入/换出；ARM64 应用库单元占 93.9 分钟。
当前 Intel 目标也运行在 ARM macos-latest 上，需修正 runner 选择。
RPM 已使用 gzip level 1 但仍超时，上游 tauri#11478、rpm-rs/rpm-rs#297 确认旧 gzip 路径
有严重性能问题；不能继续靠延长超时解决。

## 步骤 -> 验证方式

1. 已完成：收集各系统耗时与官方实现 → 核对步骤时间、Cargo timings、内存及上游问题。
2. 已实现：RPM 使用 Zstd level 1，并单独使用官方 CLI 2.12.5 的 rpm 0.30；旧 rpm 0.16.1
   的 Sha256Writer 在短写时反复散列整个剩余缓冲，Zstd 单独无法解决。优先打 AppImage，
   RPM 限时 30 分钟。其他格式复用已安装 2.11.4 CLI；每分钟记录资源、结束时输出实际耗时。
3. 已实现：两种 macOS 目标使用标准 `macos-15-intel` runner；Windows/Win7 仅将应用
   package 的 opt-level=1、codegen-units=16，与既有 Linux/macOS 方案对齐；运行时依赖保持原配置。
4. 已验证：Actions 从同发布、同 source、SHA-256 与 minisign 均通过的 ARM64 DEB 恢复
   已编译应用和资源，不重新编译，重打 AppImage/RPM；检查解包后的代码段与输入一致，再签名上传。
   Cargo 构建缓存预热随主线工具变更执行，尚未实测新参数的完整各平台构建耗时。

ARM64 重打包 run **44020624567 成功**：完整流程约 6 分钟，官方 bundle 阶段
DEB 16.8 秒（修正控制字段后该步骤共 31 秒）、AppImage 126.5 秒、RPM 19.7 秒。
与原 RPM 110.3 分钟相比瓶颈已消除。RPM 由 Ubuntu 22.04 自带包管理器在隔离根目录
实际安装成功，未关闭摘要校验；三种包重新验证 minisign、大小及提取后的应用代码段并上传。
桌面显示名和应用资源目录保留“原助理”，DEB/RPM 包管理标识固定为 iyw-claw。

普通 Windows、Intel/ARM macOS 同源码对比 run **44018664247** 仍在运行，不能将配置
变更宣称为这三端已测得固定倍数加速。Win7 两种架构的应用级参数已调整且编译计划检查通过，
本轮没有重新编译原来已签名的 Win7 0.1.350 应用。

定向检查：修改的 Node 文件语法、工作流 YAML 解析和 diff 检查通过；Win7 两架构编译计划
保留自建标准库参数，资源观测器成功/非零退出路径检查通过。未运行本机桌面构建或项目测试。

缓存调查：Linux x64 的 Rust 命中为零，macOS ARM64 已约 98% 命中但应用库仍用时 94 分钟，
不能仅靠扩大缓存解决；仓库当时只有 3 个有效缓存、合计约 12 GiB。暂不改配额或新增计费资源。

当前 0.1.350 运行中的任务不能热更新工作流；不取消或重复完整构建，修复走独立验证。
不降低应用功能，不使用无签名制品，不更改已发布标签，不启用付费 larger runner。

后续回读：原 Intel 编译最终 151.2 分钟；旧 ARM64 RPM 最终 110.3 分钟，随后
AppImage 因缺少 xdg-mime 失败，现已补齐 xdg-utils/desktop-file-utils。独立打包已实测
DEB 30 秒、AppImage 114 秒。RPM 仅换 Zstd 仍慢，正验证使用 rpm 0.30 的 CLI 2.12.5。

旧 DEB 还包含非法中文 Package 字段；保留 Tauri 编译时确定的资源目录，仅修正包控制
字段为 iyw-claw，避免中文桌面显示名与 Linux 包管理标识混用，重签后再发布。
输入 DEB、输出包均验证固定公钥 minisign、摘要和提取后的应用代码段；不执行应用重编译。
