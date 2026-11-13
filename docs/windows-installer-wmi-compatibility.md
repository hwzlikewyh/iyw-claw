# Windows 安装器 WMI 兼容处理

日期：2026-11-13。

## 问题与验收

0.1.333 正式安装器在查询 `root/cimv2:Win32_Process` 时遇到
`0x80041010`（无效类），以退出码 2 中止。异常发生在应用替换事务开始前。
这能说明目标机的进程 CIM 查询不可用，不能据此断定 WMI 文件损坏的具体原因。

验收标准：

- CIM 查询失败后能够切换到原生进程查询，后续 PID 复核复用同一查询路径。
- 停止进程前继续核验用户 SID、实际路径、启动时间和 Node 入口归属。
- 进程已退出时返回空结果，避免把 `$null` 当作一条存活记录。
- 保存查询失败与原生回退原因；日志保存失败不阻止回退。
- 保留现有安装和更新入口、退出码约定以及应用替换事务。

## 方案与边界

沿用现有 NSIS 生成 PowerShell 脚本的集成方式。查询优先使用 CIM；
首次失败后，本次脚本执行改用 Windows 原生 API，不再反复尝试 CIM。
安装、检查和卸载共用查询入口。

原生辅助类通过进程句柄读取路径、创建时间和用户 SID。
同一用户的 Node 进程另读命令行，继续交给已有逻辑识别受管入口。
无法核验的安装器候选进程仍明确失败，避免按名称直接终止进程。
进程与令牌句柄通过 `using` 或 `finally` 释放。
进程句柄基于 `SafeHandleZeroOrMinusOneIsInvalid` 封装，避免要求 .NET 4.6 的
`SafeProcessHandle`，保持旧版 PowerShell 5.1 环境的兼容范围。

64 位系统选择原生 Windows PowerShell，支持检查 32 位及 64 位 Node；
32 位系统继续使用系统 PowerShell。最低要求仍为 Windows PowerShell 5.1。
原生回退仅处理安装器的进程识别，不修改系统 WMI 配置或仓库。

实施顺序：查询统一入口 → 身份校验及临时文件清理 → 静态审查 → 隔离提交并合并主分支。

## 验证记录与发布边界

- Windows PowerShell 5.1 能通过 `Add-Type -Path` 加载原生辅助类。
- 已在开发机枚举 355 个进程，并读取当前 PowerShell 的路径和用户 SID。
- 已核对 PowerShell 的 `$null` 输出会形成一条数组记录，因此空 PID 查询显式无输出返回。
- 已展开 NSIS 内的 PowerShell 代码并通过语法解析，`git diff --check` 通过。
- 已审查安装/卸载宏前缀、查询调用链、NSIS 栈平衡、日志归档与临时文件清理。
- 依仓库规则不运行测试、不新增测试文件，不在本机构建或启动桌面端。

尚需发布环境生成正式应用安装包，再在 WMI 不可用的故障机器、Win7 和 32 位系统验证。
现有 0.1.333 安装包不会因源码合并而改变；安装向导需取得包含本修复的正式应用包才会生效。

参考微软 API 文档：[SafeProcessHandle](https://learn.microsoft.com/en-us/dotnet/api/microsoft.win32.safehandles.safeprocesshandle)
与 [SafeHandleZeroOrMinusOneIsInvalid](https://learn.microsoft.com/en-us/dotnet/api/microsoft.win32.safehandles.safehandlezeroorminusoneisinvalid)。
