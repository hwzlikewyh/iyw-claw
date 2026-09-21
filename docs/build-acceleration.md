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

## 第三批：缓存瘦身与 worker 成品备份

正常 Release、GitHub 托管打包检查和 Windows 分阶段打包入口统一使用
`v1-desktop-app` 前缀。Rust 缓存只保存 `src-tauri` 的外部依赖编译结果及 Cargo
下载缓存，不再整包缓存 worker 的 `target`，也不保留应用自身的编译结果。
`sccache` 继续用于编译缓存。这样避免同时保存 worker 中间产物和独立成品，
减少多平台缓存互相挤占。切换后的应用依赖缓存需要首次重建；worker 原有
`xinghe-build-v2` 键保持不变，不因这次工作流调整强制重编。

worker 恢复顺序：

1. 按完整编译输入键恢复 Actions Cache。
2. 未命中时，查找同键、未过期的 worker artifact 并按 ID 下载。
3. 两处均不可用时，执行原有 Cargo 构建。
4. 复用或编译完成后，始终执行摘要、架构、上游身份、ABI 和资源组装验证。

artifact 只复用本仓库默认分支上 `push` / `workflow_dispatch` 运行产生的成品，
不接受 fork 或 PR 运行。默认分支的构建在 worker 校验完成后立即备份，后续应用
编译或签名失败不会使该成品失效。tag 触发的发布可读取默认分支的匹配备份，
但不会创建新的备份。调用工作流需要已有的 `actions: read` 权限。

备份包含 worker 动态库、helper 和 SHA-256 清单，先封装为 tar，保留 macOS/Linux
可执行权限。每个编译键只在没有可用备份时上传，保留 14 天，使用 artifact 存储，
不占 Actions Cache 容量；到期后可由仍有效的缓存或新构建重新生成。
查找、下载或上传服务失败会记录提示；下载失败回退缓存或编译。已下载但损坏的
归档、摘要或二进制校验失败仍会阻止发布。

第三批本身不删除远端旧缓存、不改变编译参数、签名机调度或发布顺序。旧缓存由 GitHub
自然淘汰；worker 源码、工具链或 runner 镜像变化仍会产生新键并重新编译。
macOS Intel 和 Apple Silicon 仍打包 `libiyw_xinghe_worker.dylib` 与
`iyw-xinghe-helper`，并在 app/DMG 验证中检查 worker。

验收时应分别记录首次构建、相同引擎输入的再次发布，以及 Actions Cache 未命中但
artifact 命中的构建耗时。2026-09-17 的 v0.1.211 中，macOS worker 准备约
34-36 分钟、Windows 约 50-52 分钟；这些是优化前的实测基线，不是提速后的保证。

## 第四批：编译速度优先与缩短发布等待

桌面应用库改为只生成 `rlib`，由桌面/服务器二进制链接，不再生成未分发的
`staticlib` 和 `cdylib`。独立星河 worker 仍是 `cdylib`，Windows 的 DLL、macOS
的 dylib 和 Linux 的 so 均继续随包分发。当前项目发布桌面与服务器；将来若增加
Tauri Android/iOS，需要单独配置它们要求的库产物。

正常桌面发布、候选版、修复和托管/自托管打包检查统一使用
`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=64`、`CARGO_PROFILE_RELEASE_LTO=off`。
这些参数同时用于应用和 worker。Cargo 的 `false` 仍启用 crate 内部 thin LTO，
`off` 才完全关闭；这次仍使用 Release profile，保留原有优化等级、strip 和
panic 恢复语义。Windows worker 仅构建实际分发的两个 sandbox helper，
不再顺带构建 `managed_deny_probe` 等未分发的诊断程序。

第四批改变了编译参数及 worker 构建脚本，因而覆盖第三批“无需强制重编”的前提：
第一次采用新参数时，worker 与 Rust 依赖缓存会重新建立，之后缓存及 artifact
备份均按新键复用。不同入口使用相同参数，避免同平台因 LTO 设置不同而反复构建。
本地普通构建和独立 server CI 的 profile 参数不随此次桌面 CI 优化调整。

正常 Release 的调度变化：

- Intel macOS 单独调用原有构建工作流，安装验证只等待自己的构建，不再等待
  Apple Silicon 或 Linux 重试。正式发布仍要求五个必选平台及全部原有验证成功。
- 可选 Linux ARM64 与其他平台同时开始，不再等待正式发布；改用原生
  `ubuntu-22.04-arm` 标准 runner，保留 Ubuntu 22.04 / glibc 兼容基线。
  原来的 x64 到 ARM64 交叉编译路径仍支持其他调用入口。
- ARM64 产物只上传到已创建的 release ID，不创建或提前发布 Release；该平台仍
  不阻断五平台正式发布。其完成时间不再固定叠加在正式发布时间之后。

2026-09-18 的 v0.1.213 Cargo 报告显示，macOS ARM64 应用库耗时 2661 秒，
最终 bin 仅 5.31 秒；Windows x64 worker 库耗时 1924.6 秒，应用库 1201 秒、
最终 bin 461.4 秒。本次优先处理这些编译/链接阶段，实际改善幅度须用新的报告确认。

速度优先会减少跨单元优化，可能增加包体积或影响运行时性能。安装包体积门禁、
签名及安装验证保持启用，不通过时停止发布。GitHub 仓库当前公开，标准 ARM
runner 不新增 runner 费用；硬件签名机仍只有一台，离线或排队仍会影响总耗时。

## 第五批：预构建引擎与并行编译

`Build Xinghe Workers` 在 main 的引擎源码、patches、Cargo 配置或相关准备脚本
变化时提前构建六个平台，也可手动运行。使用已有的精确编译键、成品缓存和
14 天 artifact 备份，不改变第四批编译参数或现有 worker 缓存键。

正常 Release 在创建草稿后同时启动 worker 与前端；应用在前端准备好后立即
编译，不等待 worker。worker 命中成品时只核验、重组资源和上传本次运行的
传输包。冷构建时两个 Rust 工程分属独立 runner，不争抢同机 CPU 和 Cargo 锁。

应用编译使用 `tauri.compile-only.conf.json`，仅暂时关闭打包资源复制，仍用
真实的当前前端生成 Tauri context，并保留原有平台配置和 features。
编译命令带 `--no-bundle --no-sign`。macOS/Linux 之后通过 tauri-action 的
`tauriScript` 扩展点调用官方 `tauri bundle`，恢复完整资源配置、签名和上传；
不会因恢复配置再次运行 Cargo。Windows 保持现有独立签名封装流程。

汇合时只读取当前 GitHub run 中名称包含目标架构和实际源码 SHA 的 worker
artifact。tar 保留可执行权限；恢复后要求原有完整编译键匹配，再验证文件摘要、
架构、上游身份、ABI 和 VC 运行库。消费者设置 `IYW_XINGHE_WORKER_REQUIRE_CACHE=1`，
身份不符会停止，不静默退回串行编译。生产者已结束而产物缺失时立即失败；
其他情况最多等待 90 分钟，且仍受 job 总超时约束。

worker 的独立依赖缓存只在成品缓存和备份都无法恢复时使用，前缀为
`v1-worker-dependencies`，不保留 worker 自身产物；Swatinem 在保存前清理
非依赖文件，成品先由自己的缓存和 artifact 保存。该缓存增加容量需求，
但不会让热发布每次下载完整 worker target；成品备份继续独立于 Cache 淘汰。

候选版和其他直接调用入口默认保留串行流程。只有正常 Release 显式开启
`parallel_worker` 并启动生产者。旧 tag 不含新配置/脚本时应使用新版本发布；
不能仅重跑旧 workflow 来启用新的任务图。

已对 GitHub 上现有 macOS ARM64 artifact 执行实际查询、下载和恢复验证，外层
SHA-256、清单、文件摘要、架构、版本、ABI、helper 与 tar 执行权限均通过检查。
完整并行发布与真实提速仍须在新工作流运行后比较 Cargo timings：
冷构建目标是使 worker 与应用耗时重叠，热构建目标是跳过 worker 编译。

## 缓存工具安装失败时降级

v0.1.216 的 Intel macOS worker 因下载 sccache 返回 HTTP 504，在恢复现成
worker 之前失败。独立 worker 流程现先恢复成品缓存和 artifact，仅在两者未
命中时安装可选 sccache；成品命中不需要下载编译缓存工具。

`.github/actions/setup-sccache` 固定使用当前已采用的 v0.18.0，安装失败或
可执行文件检查失败时清空 `RUSTC_WRAPPER`，后续使用普通 Cargo 编译。
正常桌面 macOS/Linux/Windows 构建也采用该可选安装入口。真实 Cargo 编译、
摘要和二进制校验失败仍然阻止发布，不因缓存工具降级而跳过。

关闭上游 action 的自动 post 统计，避免安装失败后再次调用未定义的路径；
显式统计只在 wrapper 可用时执行，统计失败不阻断发布。该调整不改变编译参数
或 worker 编译身份键，也不需要清空已有成品。独立 server 和其他旧检查入口
不在本次修复范围。已运行的 workflow 不会自动加载这次源码修改。

## 第六批：修复跨 runner 身份与应用编译瓶颈

v0.1.216 的 Windows x86 worker 在 `windows-2022` 镜像
`20260907.297.1` 编译，应用在 `20260913.307.1` 接收。同一平台的滚动镜像
构建号进入旧指纹，导致 worker 冷编译约 49 分钟后仍被消费者拒收。

成品键升级为 `xinghe-build-v3`：仅排除 `ImageVersion`，保留源码、Rust 版本和
host、target、ImageOS、显式 SDK、原生编译器和编译参数。产物摘要、架构、
上游版本、ABI、helper 和 VC 运行库验证不变。不同基础系统或编译配置仍不能混用。
此项以可运行的编译产物为缓存单位，不要求每次 runner 补丁更新都重新编译。

迁移先找 v3，再按当前环境计算精确 v2 键，恢复旧缓存或受信任的旧 artifact。
只有旧完整指纹和文件摘要均匹配时才重写为 v3 清单，并保存新缓存和备份；
不会用 v2 前缀模糊匹配不同镜像、源码或工具链。无可验证旧产物时需要一次冷编译。
显式变更原生工具链配置仍使新键失效；需要重编同输入成品时须同时清理缓存和备份。

正常并行发布在编译应用前先检查当前 target 的 worker 生产者。已失败且没有
产物时立即停止；仍在运行则继续并行编译，不增加等待依赖。应用编译结束后立即
归档 Cargo timings，不必等签名或安装包完成；报告上传失败不阻断产物构建。

v0.1.216 的 Windows x64 应用库耗时 1770.74 秒，最终 bin 为 16.57 秒；
Linux x64 分别为 584.72 秒和 5.68 秒。两种 macOS 在取消旧失败发布前仍在
主程序编译阶段，持续约 61 分钟。因此正常并行桌面编译和 Windows 分阶段编译
使用 Cargo 命令行覆盖 `profile.release.package.iyw-claw.opt-level=1`，仅降低
应用 crate 的优化等级，不改变外部依赖、worker 参数或本地/服务器默认 profile。
这是编译速度与运行性能/包体积的取舍，实际提速仍须比较新一轮 timings，不能
将参数变化等同于已证明的“最快”。现有包体积和安装验证门禁继续生效。

Linux ARM64 的 DEB 在约 20 秒完成，RPM 此后约 49 分钟仍未结束，底层停滞原因
尚未确认。DEB 和 RPM 现分别打包上传，预编译打包先校验大小再返回 tauri-action。
DEB 阶段限时 20 分钟，RPM 15 分钟且不整包重试；macOS/Linux x64 打包上传
限时 30 分钟。非并行入口包含 Cargo 编译，保留较长的 100 分钟阶段预算。
可选 ARM64 的 RPM 失败仍明确显示，已上传 DEB 不再随其一起丢失。

## macOS 长编译与 Linux ARM64 失联（2026-09-21）

实测基线：v0.1.226 macOS ARM64 应用编译 96.5 分钟，打包 2.6 分钟。
Cargo timings 中应用库耗时 5410.28 秒，其中代码生成 4532.04 秒；依赖准备
约 6 分钟。v0.1.227 macOS 两架构均在应用编译期间达到 120 分钟 job 上限。
两次发布的 Linux ARM64 均被 GitHub 标记为 runner 失联，尚无内核 OOM 证据。

两次 macOS 日志均出现 sccache 服务退出后回退本地编译。sccache v0.18.0
默认在 600 秒没有新请求后开始退出，最多只等活动请求 10 秒；大型 crate 可能
超过这个窗口。共享 setup 入口现在设置 `SCCACHE_IDLE_TIMEOUT=0` 和
`SCCACHE_CLIENT_SIDE=1`，使编译留在客户端，服务只负责缓存，避免空闲退出
中断长编译。服务重启后的统计会重置，不能据此认定应用库不可缓存。

正常发布的并行应用编译通过 `.github/scripts/release/compile-desktop.sh` 执行：

- macOS 两架构、Linux ARM64 使用 `CARGO_BUILD_JOBS=2`，仅应用 package 使用
  `opt-level=1`、`codegen-units=16`。这覆盖第四批对应用使用 64 单元的约定；
  依赖及 worker 保持原参数，包级参数只传给本次 Cargo，不进入 worker 身份键。
  Linux x64 继续使用主分支已有的应用 `opt-level=1`，不调整其并发或代码生成单元。
- 每 60 秒输出内存、swap、内存压力和占用最高的进程名，不记录命令参数。
  `/usr/bin/time` 输出编译峰值资源；非零退出时尽力收集 Linux OOM 记录，
  诊断失败不覆盖实际编译退出码。后台采样在编译退出时停止。
- 编译结束后立即归档 Cargo timings 和资源日志；后续 worker 等待或打包失败
  不影响已上传报告。runner 整机失联时无法保证 artifact 上传，但失联前已输出
  的采样可在 Actions 日志中排查。
- Rust 依赖缓存开启 `cache-on-failure`，runner 仍在线时尽力保留已编译依赖。
  不缓存应用成品，也不依赖缓存替代源码构建或签名校验。

这是针对已观察到的长编译和资源风险的缓解；OOM 尚未证实，实际提速和失联
是否消失需在新的 macOS/Linux ARM64 运行中比较。低优化级别也影响在应用内
实例化的泛型，可能影响运行性能和包体积，原有体积、签名及安装门禁继续生效。
本机 Windows 仅能完成 workflow、脚本语法和静态调用链验证，不代表目标平台
构建已通过。串行候选版入口暂不采用该应用 profile 覆盖。

依据：

- [v0.1.226 macOS ARM64](https://github.com/hwzlikewyh/iyw-claw/actions/runs/35550257838/job/106183903189)
- [v0.1.227 Linux ARM64](https://github.com/hwzlikewyh/iyw-claw/actions/runs/35567950494/job/106234067184)
- [sccache v0.18.0 配置](https://github.com/mozilla/sccache/blob/v0.18.0/docs/Configuration.md)
- [sccache 服务退出逻辑](https://github.com/mozilla/sccache/blob/v0.18.0/src/server.rs)
- [Cargo package profile 覆盖及泛型](https://doc.rust-lang.org/cargo/reference/profiles.html#overrides)
