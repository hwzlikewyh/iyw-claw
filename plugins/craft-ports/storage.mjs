import {
  mkdir,
  readFile,
  realpath,
  lstat,
  rename,
  unlink,
  open,
} from "node:fs/promises"
import { randomUUID } from "node:crypto"
import path from "node:path"

const MAX_STATE_BYTES = 24 * 1024 * 1024
const MAX_CHANGES = 512
const MAX_KEY_LENGTH = 256
const MAX_ENTRIES = 2048
const EMPTY_STATE = { revision: "", entries: [] }

export function installEditorStorage(server, scope, schema) {
  const queues = new Map()
  const hidden = { ui: { visibility: ["app"] } }
  server.registerTool(
    "_iyw_state_read",
    { inputSchema: {}, annotations: { readOnlyHint: true }, _meta: hidden },
    async () => reply(await readState(await stateFile(scope.id())))
  )
  server.registerTool(
    "_iyw_state_commit",
    {
      inputSchema: {
        revision: schema.string().max(64),
        changes: schema
          .array(
            schema.object({
              store: schema.enum(["preferences", "presets"]),
              key: schema.string().min(1).max(MAX_KEY_LENGTH),
              value: schema.string().nullable(),
            })
          )
          .max(MAX_CHANGES),
      },
      annotations: { readOnlyHint: false, destructiveHint: false },
      _meta: hidden,
    },
    async (input) => {
      const id = scope.id(),
        previous = queues.get(id) ?? Promise.resolve()
      const next = previous.catch(() => {}).then(() => commit(id, input))
      queues.set(id, next)
      try {
        return reply(await next)
      } finally {
        if (queues.get(id) === next) queues.delete(id)
      }
    }
  )
}

async function stateFile(id) {
  if (!/^[1-9][0-9]{0,9}$/.test(id))
    throw Error("Trusted conversation context is required for editor storage")
  if (!process.env.IYW_PLUGIN_DATA_DIR)
    throw Error("Managed plugin data directory is required")
  const root = await realpath(process.env.IYW_PLUGIN_DATA_DIR),
    directory = path.join(root, "editor-state")
  await mkdir(directory, { recursive: true })
  if ((await realpath(directory)) !== directory)
    throw Error(
      "Editor state directory may not redirect outside managed storage"
    )
  return path.join(directory, `${id}.json`)
}

async function readState(file) {
  const info = await lstat(file).catch((error) => {
    if (error.code === "ENOENT") return null
    throw error
  })
  if (!info) return EMPTY_STATE
  if (!info.isFile() || info.isSymbolicLink() || info.size > MAX_STATE_BYTES)
    throw Error("Editor state file is invalid or exceeds 24 MiB")
  const bytes = await readFile(file)
  if (bytes.length > MAX_STATE_BYTES) throw Error("Editor state exceeds 24 MiB")
  const value = JSON.parse(bytes.toString("utf8"))
  if (
    typeof value.revision !== "string" ||
    value.revision.length > 64 ||
    !Array.isArray(value.entries) ||
    value.entries.length > MAX_ENTRIES ||
    value.entries.some((entry) => !validEntry(entry)) ||
    new Set(
      value.entries.map((entry) => JSON.stringify([entry.store, entry.key]))
    ).size !== value.entries.length
  )
    throw Error("Editor state is corrupt; the existing file has been retained")
  return value
}

async function commit(id, input) {
  const file = await stateFile(id),
    current = await readState(file)
  if (current.revision !== input.revision)
    throw Error("Editor state changed; reopen the page before retrying")
  const entries = new Map(
    current.entries.map((entry) => [
      JSON.stringify([entry.store, entry.key]),
      entry,
    ])
  )
  for (const change of input.changes) {
    if (!validEntry({ ...change, value: change.value ?? "" }))
      throw Error("Invalid editor storage entry")
    const key = JSON.stringify([change.store, change.key])
    if (change.value === null) entries.delete(key)
    else entries.set(key, change)
  }
  const next = { revision: randomUUID(), entries: [...entries.values()] }
  if (next.entries.length > MAX_ENTRIES)
    throw Error("Editor storage exceeds 2048 records; remove unused presets")
  const bytes = Buffer.from(JSON.stringify(next))
  if (bytes.length > MAX_STATE_BYTES)
    throw Error(
      "Editor preset storage exceeds 24 MiB; reduce the custom preset collection"
    )
  await replaceFile(file, bytes)
  return { revision: next.revision }
}

async function replaceFile(file, bytes) {
  const temporary = path.join(
    path.dirname(file),
    `.pending-${randomUUID()}.json`
  )
  const handle = await open(temporary, "wx", 0o600)
  try {
    await handle.writeFile(bytes)
    await handle.sync()
    await handle.close()
    await rename(temporary, file)
  } finally {
    await handle.close().catch(() => {})
    await unlink(temporary).catch((error) => {
      if (error.code !== "ENOENT") throw error
    })
  }
}

function validEntry(value) {
  return (
    value &&
    ["preferences", "presets"].includes(value.store) &&
    typeof value.key === "string" &&
    value.key.length > 0 &&
    value.key.length <= MAX_KEY_LENGTH &&
    typeof value.value === "string" &&
    (value.store === "preferences" ||
      (/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(
        value.value
      ) &&
        Buffer.from(value.value, "base64").toString("base64") === value.value))
  )
}

function reply(value) {
  return {
    content: [{ type: "text", text: "Managed editor state read or persisted" }],
    structuredContent: value,
  }
}
