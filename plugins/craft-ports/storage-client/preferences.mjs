import { preferenceStatus } from "./status.mjs"

const MAX_CHANGES = 512

export function managedPreferences(
  driver,
  preferenceKey = "photocraft.preferences"
) {
  const values = new Map(
    driver.read("preferences").map((entry) => [entry.key, entry.value])
  )
  const state = { values, pending: new Map(), ready: false, running: null }
  const saved = values.get(preferenceKey)
  const status = preferenceStatus(() => void drain(state, driver, status))
  const storage = Object.create(Storage.prototype)
  Object.defineProperty(storage, "length", { get: () => values.size })
  Object.assign(storage, methods(state, driver, status))
  return {
    storage,
    saved,
    resume() {
      if (saved !== undefined) {
        values.set(preferenceKey, saved)
        state.pending.delete(preferenceKey)
      }
      state.ready = true
      return drain(state, driver, status)
    },
    flush: async () => {
      await drain(state, driver, status)
      if (state.pending.size) throw Error("Editor settings remain unsaved")
    },
  }
}

function methods(state, driver, status) {
  const update = (key, value) => {
    key = String(key)
    if (value === null) state.values.delete(key)
    else state.values.set(key, value)
    state.pending.set(key, value)
    if (state.ready) void drain(state, driver, status)
  }
  return {
    key: (index) => [...state.values.keys()][index] ?? null,
    getItem: (key) => state.values.get(String(key)) ?? null,
    setItem: (key, value) => update(key, String(value)),
    removeItem: (key) => update(key, null),
    clear: () => {
      for (const key of state.values.keys()) update(key, null)
    },
  }
}

function drain(state, driver, status) {
  if (!state.ready || state.running || !state.pending.size) return state.running
  status("saving")
  let failed = false
  state.running = persist(state, driver)
    .then(() => status("saved"))
    .catch(() => {
      failed = true
      status("failed")
    })
    .finally(() => {
      state.running = null
      if (!failed && state.pending.size) void drain(state, driver, status)
    })
  return state.running
}

async function persist(state, driver) {
  while (state.pending.size) {
    const entries = [...state.pending].slice(0, MAX_CHANGES)
    await driver.write(
      entries.map(([key, value]) => ({ store: "preferences", key, value }))
    )
    for (const [key, value] of entries)
      if (state.pending.get(key) === value) state.pending.delete(key)
  }
}
