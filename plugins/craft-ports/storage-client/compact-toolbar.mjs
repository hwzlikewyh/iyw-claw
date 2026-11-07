export function compactToolbar(actions) {
  const root = document.createElement("nav")
  root.id = "iyw-compact-toolbar"
  root.setAttribute("aria-label", english() ? "Editor view" : "编辑器视图")
  const style = document.createElement("style")
  style.textContent = `#iyw-compact-toolbar{display:none;position:fixed;inset:0 0 auto;height:52px;box-sizing:border-box;gap:6px;align-items:center;padding:4px 8px;background:#242424;color:white;font:14px sans-serif;z-index:999}
.iyw-compact #iyw-compact-toolbar{display:flex}.iyw-compact #craft-stage{top:52px}
#iyw-compact-toolbar button{min-height:44px;min-width:44px;padding:4px 10px;border:1px solid #777;border-radius:4px;background:#333;color:white;font:inherit;cursor:pointer}#iyw-compact-toolbar button[aria-pressed=true]{background:#eee;color:#222}#iyw-compact-toolbar button:disabled{opacity:.6}#iyw-compact-toolbar button:focus-visible{outline:2px solid white;outline-offset:1px}
#iyw-compact-toolbar [role=alert]{position:absolute;top:52px;left:0;right:0;padding:8px;background:#8b2020}`
  document.head.appendChild(style)
  const buttons = ["menu", "panels", "options", "fit"].map((name) => {
    const button = document.createElement("button")
    button.type = "button"
    button.dataset.action = name
    button.addEventListener("click", () => actions[name]())
    root.appendChild(button)
    return button
  })
  const failure = document.createElement("span")
  failure.role = "alert"
  failure.hidden = true
  root.appendChild(failure)
  document.body.appendChild(root)
  return (state) => render({ root, buttons, failure }, state)
}

function render(elements, state) {
  document.documentElement.classList.toggle(
    "iyw-compact",
    state.narrow || state.error
  )
  const en = english()
  const labels = en
    ? ["Menu", state.panels ? "Canvas" : "Panels", "Options", "Fit canvas"]
    : ["菜单", state.panels ? "返回画布" : "面板", "选项", "适合画布"]
  elements.buttons.forEach((button, index) => {
    button.textContent = labels[index]
    button.disabled = state.busy
    if (index === 1 || index === 2)
      button.setAttribute(
        "aria-pressed",
        String(index === 2 ? state.options : state.panels)
      )
  })
  elements.failure.hidden = !state.error
  elements.failure.textContent = en
    ? "View change failed. Retry using the controls above."
    : "视图调整失败，请用上方按钮重试。"
}

function english() {
  return document.documentElement.lang.startsWith("en")
}
