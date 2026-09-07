import { browserApi } from "./browser-api"

export async function closeHiddenBrowserWindow(windowLabel: string) {
  const state = await browserApi.state()
  const registered = state.hosts.some(
    (host) => host.windowLabel === windowLabel
  )
  // 已接管页签的 host 使用既有保留流程；尚未注册的窗口可直接关闭。
  if (registered) await browserApi.closeWindowPreservingTabs(windowLabel)
  else await browserApi.closeWindow(windowLabel)
}
