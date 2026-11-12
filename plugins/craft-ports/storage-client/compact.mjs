import { compactToolbar } from "./compact-toolbar.mjs"
import { compactMenu } from "./compact-menu.mjs"

const COMPACT_VIEW = "(max-width: 639px)"

export function installCompactEditor(call) {
  const media = matchMedia(COMPACT_VIEW)
  const state = {
    compact: false,
    panels: false,
    options: false,
    saved: null,
    busy: false,
    error: false,
    narrow: media.matches,
  }
  const queue = viewQueue({ state, call, media, render: () => render(state) })
  const menu = compactMenu((name, args) => invoke(call, name, args))
  const toggle = (key) =>
    queue.schedule(async () => {
      await setPanels(call, {
        [key === "panels" ? "dock" : "options_bar"]: !state[key],
      })
      state[key] = !state[key]
    })
  const render = compactToolbar({
    menu: () => void menu.open(),
    panels: () => toggle("panels"),
    options: () => toggle("options"),
    fit: () =>
      queue.schedule(() => invoke(call, "ui_set", { fields: { fit: true } })),
  })
  const changed = () => {
    menu.close()
    queue.schedule()
  }
  media.addEventListener("change", changed)
  queue.schedule()
  return {
    pause: () => {
      menu.close()
      return queue.pause()
    },
    resume: () => {
      queue.resume()
      queue.schedule()
    },
  }
}

function viewQueue({ state, call, media, render }) {
  let queue = Promise.resolve(),
    stopped = false
  return {
    pause: async () => {
      stopped = true
      await queue
      state.busy = true
      render()
    },
    resume: () => {
      stopped = false
      state.busy = false
      render()
    },
    schedule(action) {
      queue = queue.then(async () => {
        if (stopped) return
        state.busy = true
        state.narrow = media.matches
        render()
        try {
          await sync(call, state, media.matches)
          if (media.matches && action) await action()
          state.error = false
        } catch {
          state.error = true
        } finally {
          state.busy = false
          render()
        }
      })
    },
  }
}

async function sync(call, state, compact) {
  if (compact === state.compact && !state.error && (compact || !state.saved))
    return
  if (!compact && !state.saved) {
    state.compact = false
    return
  }
  if (compact) {
    const ui = await invoke(call, "ui_inspect", {})
    state.saved ??= Object.fromEntries(
      ["dock", "rail", "options_bar"].map((key) => [key, ui.panels[key]])
    )
    await setPanels(call, { dock: false, rail: false, options_bar: false })
    state.panels = false
    state.options = false
  } else {
    await setPanels(call, state.saved)
    state.saved = null
  }
  state.compact = compact
}

function setPanels(call, panels) {
  return invoke(call, "ui_set", { fields: { panels } })
}

async function invoke(call, name, args) {
  const reply = await call({ name, arguments: args })
  if (reply.isError) throw Error("Editor view operation failed")
  const text = reply.content?.find((item) => item.type === "text")?.text
  return text ? JSON.parse(text) : null
}
