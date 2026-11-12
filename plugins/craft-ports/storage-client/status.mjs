export function preferenceStatus(retry) {
  let notice
  return (state) => {
    notice ??= createNotice(retry)
    notice.hidden = state === "saved"
    notice.style.display = state === "saved" ? "none" : "flex"
    notice.setAttribute("role", state === "failed" ? "alert" : "status")
    notice.querySelector("span").textContent = english()
      ? state === "failed"
        ? "Settings could not be saved. Keep this tab open and retry."
        : "Saving settings… Keep this tab open until saving finishes."
      : state === "failed"
        ? "编辑器设置保存失败，请保留当前标签页并重试。"
        : "正在保存编辑器设置，请保持当前标签页，直到保存完成。"
    const button = notice.querySelector("button")
    button.hidden = state !== "failed"
    button.textContent = english() ? "Retry saving" : "重试保存"
  }
}

function english() {
  return document.documentElement.lang.toLowerCase().startsWith("en")
}

function createNotice(retry) {
  const notice = document.createElement("div")
  notice.id = "iyw-storage-status"
  notice.setAttribute("aria-live", "polite")
  notice.style.cssText =
    "position:fixed;bottom:0;left:0;right:0;z-index:99999;display:flex;align-items:center;gap:12px;padding:10px 12px;background:#242424;color:#fff;font:13px sans-serif"
  const text = document.createElement("span")
  const button = document.createElement("button")
  button.type = "button"
  button.style.cssText =
    "padding:5px 10px;border:1px solid #aaa;border-radius:4px;background:#fff;color:#222;cursor:pointer;font:inherit"
  button.addEventListener("click", retry)
  notice.append(text, button)
  document.body.appendChild(notice)
  return notice
}
