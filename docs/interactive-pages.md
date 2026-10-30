# 会话右侧交互页面

`show_interactive_html` 接受 HTML 文档或 HTTP/HTTPS 网页地址，展示位置为工作区右侧面板。调用不会打开展示 Dialog；桌面网页使用原生 Wry 子 WebView，不启动受管 Chrome 或独立浏览器窗口。

## 调用

`title` 必填；`html` 和 `url` 必须恰好提供一个。

```json
{
  "title": "本地页面预览",
  "url": "http://localhost:3000",
  "wait_for_response": false
}
```

HTML 调用保持兼容：

```json
{
  "title": "选择方案",
  "html": "<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><button onclick=\"iyw.submit({choice:'A'}).catch(e=>alert(e.message))\">选择 A</button></html>",
  "wait_for_response": true
}
```

- `html` 最多 256 KiB，标题最多 120 字符，提交结果最多 64 KiB。
- `url` 最多 8192 UTF-8 字节，只接受无用户名、密码的 HTTP/HTTPS 地址。
- `wait_for_response` 默认 `false`。URL 页面仅浏览，不注入 `iyw.submit`，传 `true` 会返回参数错误。
- HTML 在原有沙箱 iframe 中运行，自包含脚本和样式可用；保留原有 CSP、MessageChannel、用户提交及取消流程。
- 等待反馈的 HTML 页面可使用标准 `<form>` 提交；宿主会拦截未被页面处理的提交，并把命名控件值发送给 Agent。同名字段以数组返回；文件控件不支持此兜底。复杂交互仍可在用户操作时调用 `iyw.submit(data)`。等待页面的沙箱允许表单事件，CSP 继续禁止表单导航。
- 返回 `presented` 表示宿主已接受展示请求，不代表网页已经加载或视觉效果已经验证。

## 右侧操作

网页提供地址栏、前进/后退、刷新/停止、页签、缩放、页内查找、桌面/窄屏尺寸切换、开发者工具、下载记录、复制地址，以及用户主动点击的系统浏览器入口。

选择元素会高亮鼠标指向的元素，点击后把 URL、标签、选择器和文本加入当前对话输入框。按 Escape 或再次点击选择按钮取消；切换或关闭页面也会取消选择。

截图可直接加入输入框，也可在右侧静态截图上框选裁剪、添加矩形/箭头标注、撤销或清空，并附说明后加入输入框。截图编辑不弹 Dialog。截图与页面内容不会自动发送给代理；输入框接收失败时会提示，避免把未送达内容标为成功。

下载保存在系统下载目录下的 `interactive-pages/<唯一目录>/`，记录显示进行中、成功和失败状态，完成后可打开或定位文件。

新页面自动展开右侧。切换页签保留已挂载页面；关闭面板、关闭页签或切换会话释放原生视图。重新打开可能重置未提交内容。历史预览不会向过去的工具调用提交答案。子会话入口携带子会话 connectionId，将反馈送回原请求。

地址栏和前进/后退状态定期从原生引擎同步，包括 SPA 使用 History API 的地址变化。Windows 为主视图保留独立窗口钩子，避免关闭 Wry 子视图后主界面不再跟随窗口调整尺寸。

## 平台与边界

桌面网页不获得 Tauri 应用 IPC 权限，浏览数据与主应用的数据目录分开。原生页面只提供用户操作和内容交接，不开放代理自动操控权限。主窗口销毁时清理原生子视图。

浏览器/服务器客户端使用受限 iframe 预览。目标网站的 CSP 或 X-Frame-Options 可能禁止嵌入，这时可以使用系统浏览器入口；原生截图、元素选择和开发者工具仅在桌面模式提供。

实现入口：

- 工具 schema：`src-tauri/src/acp/builtin_mcp/interaction_tools.rs`。
- 会话协议：`src-tauri/src/acp/interactive_html.rs`。
- 原生视图：`src-tauri/src/native_page/`。
- 右侧面板：`src/components/chat/interactive-page-panel.tsx`。
- 工作区状态：`src/contexts/interactive-page-context.tsx`。

## 验证记录（2026-10-13）

- Windows 主项目 `cargo check --offline --lib --bin iyw-claw` 通过，原生模块独立编译与注入脚本语法检查通过；存在仓库既有 Rust warning。
- TypeScript 类型检查与页面相关代码的定向 ESLint 通过。输入框的既有两处格式错误及两条 voice Hook warning 未在本任务中修改。
- 前端生产构建未完成：Turbopack CSS 工作进程超时；Webpack 重试长时间停在编译阶段后主动停止，未声称静态导出成功。
- macOS 交叉编译因本机缺少 C 交叉编译器停止；Linux 离线检查缺少缓存中的 `xkeysym`，未验证对应平台编译结果。
- 按仓库规则未新增或执行测试；未完成真实桌面交互验证，Linux 原生子视图的 Wayland 路径也尚未验证。
