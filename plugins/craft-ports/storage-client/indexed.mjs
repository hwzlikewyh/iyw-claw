import { encode, decode } from "./driver.mjs"

const DATABASE = "photocraft.presets",
  STORE = "files"

export function presetDatabase(driver) {
  const lifecycle = { pending: new Set(), failures: new Set() }
  const rows = new Map(
    driver
      .read("presets")
      .map((entry) => [entry.key, new Blob([decode(entry.value)])])
  )
  const database = Object.create(IDBDatabase.prototype)
  const fields = {
    name: DATABASE,
    version: 1,
    objectStoreNames: [STORE],
    close() {},
    createObjectStore: () => {
      throw new DOMException(
        "The managed preset store already exists",
        "ConstraintError"
      )
    },
    transaction: (name, mode = "readonly") =>
      track(transaction({ name, mode, rows, driver }), lifecycle),
  }
  Object.defineProperties(
    database,
    Object.fromEntries(
      Object.entries(fields).map(([key, value]) => [key, { value }])
    )
  )
  const factory = Object.assign(Object.create(IDBFactory.prototype), {
    open(name, version = 1) {
      const result = eventObject(IDBOpenDBRequest.prototype, {
        result: database,
        error: null,
        transaction: null,
      })
      setTimeout(() => {
        if (name !== DATABASE || version !== 1)
          fail(result, "Only the managed editor preset database is supported")
        else result.dispatchEvent(new Event("success"))
      }, 0)
      return result
    },
  })
  return { factory, flush: () => flushTransactions(lifecycle) }
}

function track({ tx, state }, lifecycle) {
  let settle
  const pending = new Promise((resolve) => {
    settle = resolve
  })
  lifecycle.pending.add(pending)
  const finish = (success) => {
    if (tx.mode === "readwrite") {
      for (const key of state.changes.keys()) {
        if (success) lifecycle.failures.delete(key)
        else lifecycle.failures.add(key)
      }
    }
    lifecycle.pending.delete(pending)
    settle()
  }
  tx.addEventListener("complete", () => finish(true), { once: true })
  tx.addEventListener("abort", () => finish(false), { once: true })
  return tx
}

async function flushTransactions(lifecycle) {
  while (lifecycle.pending.size) await Promise.all([...lifecycle.pending])
  if (lifecycle.failures.size)
    throw Error(
      "Brush changes remain unsaved; retry saving presets before closing"
    )
}

function transaction({ name, mode, rows, driver }) {
  if (name !== STORE || !["readonly", "readwrite"].includes(mode))
    throw new DOMException("Unsupported editor preset store", "NotFoundError")
  const tx = eventObject(IDBTransaction.prototype, { error: null, mode })
  const state = {
    active: true,
    committing: false,
    pending: 0,
    generation: 0,
    changes: new Map(),
    rows,
    driver,
  }
  const store = objectStore(tx, state)
  Object.assign(tx, {
    objectStore: (key) => {
      if (key !== STORE)
        throw new DOMException("Preset store not found", "NotFoundError")
      return store
    },
    abort: () => {
      if (!state.active || state.committing)
        throw new DOMException(
          "Preset commit has already started",
          "InvalidStateError"
        )
      abort(tx, state, "Editor preset transaction was aborted")
    },
  })
  scheduleCompletion(tx, state)
  return { tx, state }
}

function objectStore(tx, state) {
  const store = Object.create(IDBObjectStore.prototype)
  return Object.assign(store, {
    openCursor() {
      const entries = [...state.rows],
        request = eventObject(IDBRequest.prototype, {
          result: null,
          error: null,
        })
      let index = 0
      const advance = () =>
        enqueue(tx, state, async () => {
          const entry = entries[index++]
          request.result = entry
            ? Object.defineProperties(
                Object.create(IDBCursorWithValue.prototype),
                {
                  key: { value: entry[0] },
                  value: { value: entry[1] },
                  continue: { value: advance },
                }
              )
            : null
          request.dispatchEvent(new Event("success"))
        })
      advance()
      return request
    },
    put(value, key) {
      return mutate({ tx, state, key, value })
    },
    delete(key) {
      return mutate({ tx, state, key, value: null })
    },
  })
}

function mutate({ tx, state, key, value }) {
  if (
    tx.mode !== "readwrite" ||
    !(value === null || value instanceof Blob) ||
    typeof key !== "string"
  )
    throw new DOMException("Invalid managed preset mutation", "DataError")
  const result = eventObject(IDBRequest.prototype, {
    result: undefined,
    error: null,
  })
  enqueue(tx, state, async () => {
    state.changes.set(key, value)
    result.result = key
    result.dispatchEvent(new Event("success"))
  })
  return result
}

function enqueue(tx, state, operation) {
  if (!state.active || state.committing)
    throw new DOMException(
      "Preset transaction is inactive",
      "TransactionInactiveError"
    )
  state.pending++
  state.generation++
  setTimeout(async () => {
    if (!state.active) return
    try {
      await operation()
    } catch (error) {
      abort(tx, state, String(error))
      return
    }
    state.pending--
    scheduleCompletion(tx, state)
  }, 0)
}

function scheduleCompletion(tx, state) {
  const generation = state.generation
  setTimeout(() => {
    if (generation === state.generation && !state.pending)
      void complete(tx, state)
  }, 0)
}

async function complete(tx, state) {
  if (!state.active || state.committing) return
  state.committing = true
  try {
    const changes = []
    for (const [key, value] of state.changes)
      changes.push({
        store: "presets",
        key,
        value:
          value === null
            ? null
            : encode(new Uint8Array(await value.arrayBuffer())),
      })
    if (changes.length) await state.driver.write(changes)
    if (!state.active) return
    for (const [key, value] of state.changes) {
      if (value === null) state.rows.delete(key)
      else state.rows.set(key, value)
    }
    state.active = false
    tx.dispatchEvent(new Event("complete"))
  } catch (error) {
    abort(tx, state, String(error))
  }
}

function abort(tx, state, message) {
  if (!state.active) return
  state.active = false
  tx.error = new DOMException(message, "AbortError")
  tx.dispatchEvent(new Event("abort"))
}

function fail(request, message) {
  request.error = new DOMException(message, "NotSupportedError")
  request.dispatchEvent(new Event("error"))
}
function eventObject(prototype, properties) {
  const value = new EventTarget()
  Object.setPrototypeOf(value, prototype)
  Object.defineProperties(
    value,
    Object.fromEntries(
      Object.entries(properties).map(([key, value]) => [
        key,
        { value, writable: true, configurable: true },
      ])
    )
  )
  return value
}
