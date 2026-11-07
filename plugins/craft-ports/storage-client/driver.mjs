const MAX_STATE_BYTES = 24 * 1024 * 1024

export async function stateDriver(call) {
  const loaded = await request(call, "_iyw_state_read", {})
  let revision = loaded.revision,
    queue = Promise.resolve()
  const entries = entryMap(loaded)
  return {
    read: (store) =>
      [...entries]
        .map(([key, value]) => ({
          store: JSON.parse(key)[0],
          key: JSON.parse(key)[1],
          value,
        }))
        .filter((entry) => entry.store === store),
    write(changes) {
      const next = queue
        .catch(() => {})
        .then(async () => {
          if (
            new TextEncoder().encode(JSON.stringify(changes)).length >
            MAX_STATE_BYTES
          )
            throw Error("Editor storage transaction exceeds 24 MiB")
          const result = await commit(call, { revision, changes }, entries)
          revision = result.revision
          apply(entries, changes)
        })
      queue = next
      return next
    },
    flush: () => queue,
  }
}

async function commit(call, input, entries) {
  try {
    return await request(call, "_iyw_state_commit", input)
  } catch (error) {
    // 写盘成功但响应丢失时，仅回读确认结果；不重复写入或覆盖另一页面的新状态。
    const loaded = await request(call, "_iyw_state_read", {}).catch(() => null)
    if (!loaded || loaded.revision === input.revision) throw error
    const expected = new Map(entries)
    apply(expected, input.changes)
    const actual = entryMap(loaded)
    if (
      expected.size !== actual.size ||
      [...expected].some(([key, value]) => actual.get(key) !== value)
    )
      throw error
    return { revision: loaded.revision }
  }
}

function entryMap(snapshot) {
  return new Map(
    snapshot.entries.map((entry) => [
      JSON.stringify([entry.store, entry.key]),
      entry.value,
    ])
  )
}

function apply(entries, changes) {
  for (const change of changes) {
    const key = JSON.stringify([change.store, change.key])
    if (change.value === null) entries.delete(key)
    else entries.set(key, change.value)
  }
}

async function request(call, name, arguments_) {
  const result = await call({ name, arguments: arguments_ })
  if (result.isError)
    throw Error(
      result.content?.find((item) => item.type === "text")?.text ??
        "Editor state operation failed"
    )
  if (!result.structuredContent)
    throw Error("Editor storage response is invalid")
  return result.structuredContent
}

export function encode(bytes) {
  let text = ""
  for (const value of bytes) text += String.fromCharCode(value)
  return btoa(text)
}

export function decode(base64) {
  return Uint8Array.from(atob(base64), (character) => character.charCodeAt(0))
}
