(function () {
  const capability = new URLSearchParams(location.search).get("cap") || ""
  const base = new URL(location.href)
  base.search = ""
  if (!base.pathname.endsWith("/")) base.pathname += "/"
  window.officeUrl = function (value) {
    const url = new URL(value, base)
    if (url.origin !== base.origin || !url.pathname.startsWith(base.pathname)) {
      throw new Error("Document resource is outside the preview")
    }
    url.searchParams.set("cap", capability)
    return url
  }
  const status = document.getElementById("status")
  const notice = document.getElementById("notice")
  const retry = document.getElementById("retry")
  const chinese = (new URLSearchParams(location.search).get("locale") || "").startsWith("zh")
  retry.textContent = chinese ? "重试" : "Retry"
  let current = ""
  let pending = false
  let again = false
  let stopped = false
  let renderError = false
  function show(message, canRetry = false) {
    notice.hidden = false
    status.textContent = message
    retry.hidden = !canRetry
  }
  window.officeReportError = function (error) {
    if (stopped) return
    renderError = true
    show((chinese ? "预览读取失败：" : "Preview failed: ") + String(error), true)
  }
  async function refresh() {
    if (stopped) return
    if (pending) { again = true; return }
    pending = true
    try {
      const response = await fetch(window.officeUrl("status"), { cache: "no-store" })
      if (!response.ok) throw new Error(`Preview status: ${response.status}`)
      const info = await response.json()
      if (stopped) return
      if (info.generation !== current) {
        renderError = false
        show(chinese ? "正在更新预览…" : "Updating preview…")
        await window.officeLoad(window.officeUrl(`data/${info.generation}/`).toString())
        if (stopped) return
        current = info.generation
      }
      if (info.error) show((chinese ? "预览更新失败：" : "Preview update failed: ") + info.error, true)
      else if (!renderError) notice.hidden = true
    } catch (error) {
      window.officeReportError(error)
    } finally {
      pending = false
      if (again) { again = false; void refresh() }
    }
  }
  const events = new EventSource(window.officeUrl("events"))
  events.onmessage = () => void refresh()
  events.onerror = () => {
    if (!stopped) show(chinese ? "正在重新连接预览…" : "Reconnecting preview…")
  }
  // 连接重建也主动核对版本，覆盖断线期间错过的通知。
  events.onopen = () => void refresh()
  retry.onclick = () => { current = ""; void refresh() }
  window.addEventListener("pagehide", () => {
    stopped = true
    events.close()
    window.officeDispose?.()
  }, { once: true })
  void refresh()
})()
