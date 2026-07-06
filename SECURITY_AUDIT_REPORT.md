# iyw-claw 项目安全审计报告

**审计日期**: 2026-07-04  
**审计范围**: 代码库中的高风险操作

---

## 执行摘要

本报告对 iyw-claw 项目进行了全面的安全审计，重点关注以下高风险操作：
- 文件系统读写操作
- 子进程执行
- Shell 命令执行
- 动态代码执行

**总体评估**: 代码库整体设计安全，有良好的安全边界和防护机制，但存在一些需要注意的高风险操作。

---

## 1. 子进程执行风险

### 1.1 Rust 后端子进程执行

#### 1.1.1 终端命令执行 - 高风险

**文件**: `src-tauri/src/acp/terminal_runtime.rs`

**位置**: `create_terminal()` 函数 (行 305-406)

**风险详情**:
```rust
// 直接执行用户提供的命令
let mut direct = crate::process::tokio_command(&request.command);
direct.args(&request.args);
self.configure_command(&mut direct, &request);

let mut child = match direct.spawn() {
    Ok(child) => child,
    Err(err)
        if err.kind() == std::io::ErrorKind::NotFound
            && request.args.is_empty()
            && request.command.contains(char::is_whitespace) =>
    {
        // 如果找不到命令且看起来像完整 shell 命令，尝试通过 shell 执行
        let mut shell = shell_wrapped_command(&request.command);
        self.configure_command(&mut shell, &request);
        shell.spawn()?
    }
```

**风险分析**:
- ✅ 命令执行是设计功能的一部分（多智能体编码工作台）
- ⚠️ 支持完整 Shell 命令执行（通过 fallback 机制）
- ✅ 未发现用户输入未经处理直接传递的情况
- ✅ 有会话隔离机制

**Shell 包装实现**:
- Unix: `/bin/sh -c <command>` (行 593-597)
- Windows: `cmd /C start "" cmd /K <command>` (行 6624-6630)

#### 1.1.2 ACP 代理进程执行 - 中高风险

**文件**: `src-tauri/src/acp/connection.rs`

**位置**: `build_agent()` 函数

**风险详情**:
- 通过 NPX 执行多个 AI 代理（Claude Code、Codex、OpenClaw、Cline 等）
- 使用 `AcpAgent::from_args(&refs)` 生成代理进程
- 合并了环境变量和代理配置

**风险分析**:
- ✅ 代理命令来自注册表配置，而非直接用户输入
- ✅ 有环境变量合并机制
- ⚠️ 支持用户自定义代理配置

#### 1.1.3 外部终端打开功能 - 中风险

**文件**: `src-tauri/src/commands/acp.rs` (行 6603-6657)

**风险详情**:
- macOS: 通过 AppleScript 执行 `osascript -e "tell application \"Terminal\"..."`
- Windows: 通过 `cmd /C start "" cmd /K <command>`
- Linux: 尝试多个终端模拟器（x-terminal-emulator、gnome-terminal、konsole、xterm）

**安全措施**:
- ✅ macOS: 使用 `shell_single_quote()` 函数进行 shell 转义
- ✅ 路径处理有适当的转义机制

**转义函数**: (行 6664-6667)
```rust
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
```

#### 1.1.4 监管进程 - 低风险

**文件**: `src-tauri/src/supervise.rs`

**功能**: `iyw-claw-server --supervise` 自监管和重启

**风险分析**:
- ✅ 仅执行自身二进制
- ✅ 参数来自自身环境，无外部输入

### 1.2 更新系统

**文件**: `src-tauri/src/update/mod.rs`

**功能**: 自更新重新执行

**风险分析**:
- ✅ 仅执行自身二进制
- ✅ 有签名验证机制（见 `verify.rs`）

---

## 2. 文件系统操作风险

### 2.1 工作区文件操作 - 良好防护

**文件**: `src-tauri/src/acp/file_system_runtime.rs`

**安全机制**: `ensure_path_in_workspace()` 函数 (行 365-382)

```rust
fn ensure_path_in_workspace(
    path: &Path,
    workspace_root: &Path,
    workspace_root_canonical: Option<&Path>,
    for_write: bool,
) -> Result<(), FileSystemRuntimeError> {
    let root = canonical_workspace_root(workspace_root, workspace_root_canonical);
    let target = canonical_target_path(path, for_write)?;

    if !target.starts_with(&root) {
        return Err(FileSystemRuntimeError::InvalidParams(format!(
            "path is outside workspace root: {}",
            path.display()
        )));
    }
    Ok(())
}
```

**防护措施**:
- ✅ 路径规范化检查（使用 `std::fs::canonicalize`）
- ✅ 防止 `..` 路径遍历
- ✅ 写入时检查父目录存在性
- ✅ 符号链接解析后检查（防止 symlink 绕过）
- ✅ 文件大小限制（最大 16MB 读取，2MB 写入）
- ✅ 并发操作限制（最多 8 个并发）
- ✅ 操作超时（30 秒）

**原子写入**: `atomic_write_text()` 函数 (行 248-298)
- 使用临时文件 + 替换操作，防止写入中断导致文件损坏
- 保留原有文件权限
- 清理失败的临时文件

### 2.2 文件夹命令文件操作

**文件**: `src-tauri/src/commands/folders.rs`

**安全机制**: `ensure_path_in_workspace()` (行 2892-2900)

```rust
fn ensure_path_in_workspace(root: &Path, target: &Path) -> Result<(), AppCommandError> {
    let canonical_root = std::fs::canonicalize(root).map_err(AppCommandError::io)?;
    let canonical_target = std::fs::canonicalize(target).map_err(AppCommandError::io)?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(AppCommandError::invalid_input("Path is outside workspace root"));
    }
    Ok(())
}
```

**防护措施**:
- ✅ 完整的规范化路径检查
- ✅ 测试覆盖路径遍历防护
- ✅ 明确拒绝符号链接目标写入

### 2.3 文件保存命令

**文件**: `src-tauri/src/commands/file_io.rs`

**功能**: 用户选择位置保存文件

**风险分析**:
- ✅ 路径来自系统文件保存对话框（用户明确选择）
- ✅ 适用于下载/导出功能
- ⚠️ 无工作区限制（设计允许保存到任意位置）

---

## 3. 动态代码执行

### 3.1 前端动态代码执行 - 无风险

**搜索结果**: 未发现 `eval()` 或 `new Function()` 的使用

### 3.2 前端 HTML 操作 - 防护良好

**文件**: `src/lib/html-preview-inline.ts`

**安全设计**:
- ✅ 使用 `<template>` 元素进行惰性 HTML 解析（防止解析时自动加载资源）
- ✅ 输出使用 iframe sandbox 渲染
- ✅ 默认禁用脚本，脚本执行需要用户明确授权
- ✅ 工作区资源有访问控制

---

## 4. 前端 XSS 防护

### 4.1 `dangerouslySetInnerHTML` 使用

**文件 1**: `src/app/layout.tsx` (行 49, 54)

**用途**:
- 注入 CSS 防止暗色模式闪烁
- 注入外观初始化脚本

**风险分析**:
- ✅ 内容是硬编码的静态字符串
- ✅ 无用户输入
- ✅ 极低风险

**文件 2**: `src/lib/appearance-script.ts` (引用)

**用途**: 主题和字体大小初始化

**风险分析**:
- ✅ 脚本内容在构建时确定
- ✅ 无用户输入

---

## 5. 进程管理工具

**文件**: `src-tauri/src/process.rs`

**功能**: 子进程配置工具

**安全特性**:
- ✅ Windows: 支持 `.exe/.cmd/.bat` 扩展名 fallback
- ✅ 设置 UTF-8 环境变量（防止 locale 相关问题）
- ✅ Node.js 路径自动检测
- ✅ PATH 操作有去重机制

---

## 6. 安全最佳实践总结

### 6.1 已实施的安全措施

| 类别 | 措施 | 状态 |
|------|------|------|
| 文件系统 | 路径规范化 | ✅ |
| 文件系统 | 工作区边界检查 | ✅ |
| 文件系统 | 符号链接解析防护 | ✅ |
| 文件系统 | 原子写入操作 | ✅ |
| 子进程 | 优先直接 exec，fallback shell | ✅ |
| 子进程 | Shell 转义函数 | ✅ |
| XSS | 内容安全策略 | ✅ |
| XSS | Iframe sandbox | ✅ |
| 前端 | 无 eval/Function | ✅ |

### 6.2 潜在改进建议

#### 建议 1: 终端命令审计日志
**位置**: `src-tauri/src/acp/terminal_runtime.rs`

**建议**: 记录所有执行的终端命令，包括参数和工作目录，用于审计目的。

#### 建议 2: 代理执行限制
**位置**: `src-tauri/src/acp/connection.rs`

**建议**: 考虑为代理进程执行添加额外的权限限制或沙箱机制。

#### 建议 3: 依赖项审计
**建议**: 定期审计 npm/cargo 依赖项的漏洞，配置自动更新机制。

#### 建议 4: Secret 管理
**建议**: 确保 API keys 和 tokens 安全存储，不写入日志。

---

## 7. 风险评级矩阵

| 组件 | 风险等级 | 说明 |
|------|----------|------|
| 终端执行 | 高 | 固有风险，但为必要功能 |
| 代理执行 | 中高 | 执行外部代码，但来自受信任源 |
| 文件系统 | 低 | 有良好的边界检查 |
| 前端 XSS | 低 | 无明显漏洞 |
| 更新系统 | 低 | 有签名验证 |

---

## 8. 结论

iyw-claw 项目整体安全状况良好，安全架构设计合理：

1. **文件系统操作**: ✅ 良好的边界防护和规范化处理
2. **子进程执行**: ⚠️ 功能复杂但有适当防护
3. **动态代码执行**: ✅ 前端无 eval，Rust 无动态编译
4. **XSS 防护**: ✅ 使用 sandbox 和 CSP

**核心原则**: 由于这是一个编码工作台，执行代码和终端命令是核心功能，风险是固有且必要的。项目通过良好的隔离和边界检查来管理这些风险。

---

## 附录 A: 审计方法

- 静态代码分析 (grep 模式匹配)
- 关键模块深度阅读
- 安全边界验证
- 测试用例审查

---

## 附录 B: 文件清单

已审计的关键文件：
- `src-tauri/src/acp/terminal_runtime.rs`
- `src-tauri/src/acp/file_system_runtime.rs`
- `src-tauri/src/acp/connection.rs`
- `src-tauri/src/commands/acp.rs`
- `src-tauri/src/commands/folders.rs`
- `src-tauri/src/commands/file_io.rs`
- `src-tauri/src/process.rs`
- `src-tauri/src/supervise.rs`
- `src-tauri/src/update/mod.rs`
- `src/lib/html-preview-inline.ts`
- `src/app/layout.tsx`
