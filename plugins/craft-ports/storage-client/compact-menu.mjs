export function compactMenu(invoke) {
  const dialog = document.createElement("dialog")
  const state = { items: [], busy: false, generation: 0 }
  const dismiss = () => {
    ++state.generation
    if (dialog.open) dialog.close()
  }
  dialog.id = "iyw-compact-menu"
  dialog.setAttribute("aria-label", english() ? "Editor menu" : "编辑器菜单")
  dialog.style.cssText =
    "box-sizing:border-box;width:calc(100% - 24px);max-height:calc(100% - 24px);padding:12px;border:1px solid #777;border-radius:6px;background:#242424;color:white;font:14px sans-serif"
  const search = document.createElement("input")
  search.type = "search"
  search.placeholder = english() ? "Find a command" : "搜索命令"
  search.setAttribute("aria-label", search.placeholder)
  search.style.cssText =
    "box-sizing:border-box;width:100%;height:40px;padding:8px;font:inherit;color:black;background:white"
  const close = document.createElement("button")
  close.type = "button"
  close.textContent = english() ? "Back to canvas" : "返回画布"
  close.style.cssText = "min-height:44px;margin-bottom:8px;font:inherit"
  close.addEventListener("click", dismiss)
  const list = document.createElement("div")
  list.style.cssText =
    "overflow:auto;max-height:65dvh;display:grid;gap:4px;margin-top:8px"
  const status = document.createElement("p")
  status.role = "status"
  dialog.append(close, search, status, list)
  bindCancel(dialog, dismiss)
  document.body.appendChild(dialog)
  const render = () =>
    renderItems({ list, search, dialog, status, state, invoke })
  search.addEventListener("input", render)
  return menuController({
    dialog,
    search,
    list,
    status,
    state,
    render,
    dismiss,
    invoke,
  })
}

function menuController({
  dialog,
  search,
  list,
  status,
  state,
  render,
  dismiss,
  invoke,
}) {
  return {
    close: dismiss,
    async open() {
      if (dialog.open || state.busy) return
      const generation = ++state.generation
      dialog.showModal()
      search.value = ""
      list.replaceChildren()
      status.textContent = english() ? "Loading…" : "正在读取菜单…"
      try {
        const items = await invoke("control_call", { method: "ui.menu.list" })
        if (state.generation !== generation) return
        state.items = items
        status.textContent = ""
        render()
      } catch {
        if (state.generation !== generation) return
        status.textContent = english()
          ? "Menu unavailable. Close and retry."
          : "菜单读取失败，请关闭后重试。"
      }
    },
  }
}

function renderItems({ list, search, dialog, status, state, invoke }) {
  list.replaceChildren()
  const query = search.value.trim().toLowerCase()
  const matches = state.items.filter(
    (item) =>
      item.id !== "---" &&
      `${item.path.join(" / ")} ${item.label} ${item.id}`
        .toLowerCase()
        .includes(query)
  )
  for (const item of matches) {
    const button = document.createElement("button")
    button.type = "button"
    button.style.cssText =
      "min-height:44px;text-align:left;padding:8px;white-space:normal;border:1px solid #666;background:#333;color:white;font:inherit"
    button.textContent = `${item.checked ? "✓ " : ""}${item.path.join(" / ")} / ${item.label}`
    button.disabled = !item.enabled || state.busy
    button.addEventListener("click", async () => {
      const generation = state.generation
      state.busy = true
      renderItems({ list, search, dialog, status, state, invoke })
      try {
        dialog.close()
        await invoke("_iyw_user_menu", { id: item.id })
      } catch {
        if (state.generation === generation) {
          notifyFailure()
        }
      } finally {
        state.busy = false
        renderItems({ list, search, dialog, status, state, invoke })
      }
    })
    list.appendChild(button)
  }
  if (!matches.length)
    status.textContent = english() ? "No matching commands" : "没有匹配的命令"
}

function notifyFailure() {
  const notice = document.getElementById("craft-notice")
  const status = document.getElementById("craft-status")
  // 原导出提示保留取消/失败原因及重新保存入口，避免第二个弹层遮挡画布。
  if (!notice || !status || !notice.hidden) return
  status.textContent = english()
    ? "Command failed. Check the editor before retrying."
    : "操作未成功，请检查编辑器状态后重试。"
  status.dataset.error = "true"
  notice.hidden = false
}

function bindCancel(dialog, dismiss) {
  dialog.addEventListener("cancel", (event) => {
    event.preventDefault()
    event.stopPropagation()
    dismiss()
  })
}

function english() {
  return document.documentElement.lang.startsWith("en")
}
