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
  const chinese = (new URLSearchParams(location.search).get("locale") || "").startsWith("zh")
  let current = ""
  let pending = false
  let again = false
  async function refresh() {
    if (pending) { again = true; return }
    pending = true
    try {
      const response = await fetch(window.officeUrl("status"), { cache: "no-store" })
      if (!response.ok) throw new Error(`Preview status: ${response.status}`)
      const info = await response.json()
      if (info.error) {
        status.hidden = false
        status.textContent = (chinese ? "预览更新失败：" : "Preview update failed: ") + info.error
      } else if (info.generation !== current) {
        status.hidden = false
        status.textContent = chinese ? "正在更新预览…" : "Updating preview…"
        await window.officeLoad(window.officeUrl(`data/${info.generation}/`).toString())
        current = info.generation
        status.hidden = true
      } else {
        status.hidden = true
      }
    } catch (error) {
      status.hidden = false
      status.textContent = (chinese ? "预览读取失败：" : "Preview failed: ") + String(error)
    } finally {
      pending = false
      if (again) { again = false; void refresh() }
    }
  }
  const events = new EventSource(window.officeUrl("events"))
  events.onmessage = () => void refresh()
  events.onerror = () => {
    status.hidden = false
    status.textContent = chinese ? "正在重新连接预览…" : "Reconnecting preview…"
  }
  // 连接重建也主动核对版本，覆盖断线期间错过的通知。
  events.onopen = () => void refresh()
  window.addEventListener("pagehide", () => events.close(), { once: true })
  void refresh()
})()
