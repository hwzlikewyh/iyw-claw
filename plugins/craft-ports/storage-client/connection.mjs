const RECONNECT_WINDOW_MS = 55_000
const RETRY_INTERVAL_MS = 5_000
const OCCUPIED =
  "This editor already has an active panel. Close it before opening another."

export async function attachEditor(call, status, isClosed) {
  const deadline = Date.now() + RECONNECT_WINDOW_MS
  const english = document.documentElement.lang.startsWith("en")
  while (!isClosed()) {
    const result = await call({ name: "_editor_attach", arguments: {} })
    if (!result.isError) {
      status(english ? "Ready" : "就绪")
      return result.structuredContent
    }
    const message = result.content?.find((entry) => entry.type === "text")?.text
    if (message !== OCCUPIED) throw Error(message ?? "Editor connection failed")
    if (Date.now() >= deadline)
      throw Error(
        english
          ? "Another editor page is still active. Close it before reopening this page."
          : "另一个编辑器页面仍在使用，请先关闭原页面，再重新打开此页面。"
      )
    status(
      english
        ? "Waiting for the previous editor connection to close… Keep this page open."
        : "正在等待原编辑器连接释放，请先关闭原页面并保留此页面。"
    )
    await new Promise((resolve) => setTimeout(resolve, RETRY_INTERVAL_MS))
  }
  throw Error("Editor page closed before connecting")
}
