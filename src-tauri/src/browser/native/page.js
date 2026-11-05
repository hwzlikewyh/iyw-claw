(() => {
  if (window.top !== window) return
  const send = (value) => window.ipc.postMessage(JSON.stringify(value))
  const ready = () => send({ type: "ready", url: window.location.href })
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", ready, { once: true })
  } else {
    ready()
  }
  window.addEventListener("pointerdown", (event) => {
    if (event.isTrusted) send({ type: "input" })
  }, true)
  window.addEventListener("keydown", (event) => {
    if (!event.isTrusted) return
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
      event.preventDefault()
      send({ type: "find" })
    } else send({ type: "input" })
  }, true)
})()
