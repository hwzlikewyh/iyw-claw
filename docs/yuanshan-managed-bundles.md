# 远山受管运行包

远山 0.108.0 沿用 npm 交付、Node 24 与宿主侧问桥。目标为 Windows、macOS、Linux glibc 各自的 x64/ARM64，共六个平台。Linux musl、Windows 7 和 32 位 x86 不包含在这次发布范围内。

`Managed Yuanshan Bundles` 使用六个对应架构的 GitHub 原生 runner，只构建 Agent 包，不编译桌面应用，也不接受 Fusion 凭据。包、SDK 版本和官方 npm integrity 固定在打包脚本中。

验收步骤：

1. 原生安装 → Node 主版本必须为 24，OS/arch 必须与矩阵一致，npm lock 校验官方根包 integrity。
2. 启动验证 → 执行本平台 SDK 原生 CLI 的版本命令，通过宿主侧问桥完成 ACP v1 initialize 和侧问 RPC 注册检查，不发送模型 prompt。
3. 归档 → 保留 `iyw-agent-bundle.json`、完整 node_modules 与 lockfile；Unix npm bin 链接转换为包内相对入口，tar/zip 不包含符号链接。
4. 回解压验证 → 对归档解压内容重复启动检查，输出文件摘要、大小、lock 摘要和原生验证记录。
5. 发布 → 收集六份 Actions artifacts，通过 Fusion 已有镜像直传接口逐一登记，六个平台全部就绪后更新同一 Agent 版本 recipe；公开票据回下载校验后才能更新强制策略。

公开执行包与 npm runtime bundle 的结构不同，不能将上游独立二进制直接登记成 npm bundle。任何平台失败应保留旧推荐，不得静默删除目标后宣称全平台发布成功。
