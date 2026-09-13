import fs from "node:fs"
import path from "node:path"
import crypto from "node:crypto"

export const MAX_BYTES = 32 * 1024 * 1024
const MAX_FILES = 512
const MAX_DEPTH = 12
const IGNORED = new Set([".git", "node_modules", ".venv", "__pycache__"])

export const digest = (value) => crypto.createHash("sha256").update(value).digest("hex")
export const readJson = (file) => JSON.parse(fs.readFileSync(file, "utf8"))

export function writeJson(file, value) {
  const temporary = `${file}.${crypto.randomUUID()}.tmp`
  try {
    fs.writeFileSync(temporary, `${JSON.stringify(value, null, 2)}\n`, { flag: "wx" })
    fs.renameSync(temporary, file)
  } finally {
    if (fs.existsSync(temporary)) fs.unlinkSync(temporary)
  }
}

export function text(value, label) {
  if (typeof value !== "string" || !value.trim() || value.length > MAX_BYTES) {
    throw new Error(`${label} must be nonempty bounded text`)
  }
  return value
}

export function targetPath(root, relative) {
  text(relative, "target")
  if (relative.includes("\\") || path.isAbsolute(relative)) throw new Error("Use a relative target")
  const parts = relative.split("/")
  if (parts.some((part) => !part || part === ".." || part === "." || IGNORED.has(part))) {
    throw new Error("Invalid target path")
  }
  let current = root
  for (const part of parts) {
    current = path.join(current, part)
    if (fs.lstatSync(current).isSymbolicLink()) throw new Error("Symlink targets are unsupported")
  }
  return current
}

export function snapshot(root) {
  const files = []
  walk(root, "", files, 0)
  if (files.reduce((total, item) => total + item.data.length, 0) > MAX_BYTES) {
    throw new Error("Skill exceeds the snapshot budget")
  }
  files.sort((a, b) => a.name.localeCompare(b.name))
  return files
}

function walk(root, prefix, files, depth) {
  if (depth > MAX_DEPTH) throw new Error("Skill nesting exceeds the snapshot budget")
  for (const entry of fs.readdirSync(path.join(root, prefix), { withFileTypes: true })) {
    if (IGNORED.has(entry.name)) continue
    const name = prefix ? `${prefix}/${entry.name}` : entry.name
    if (entry.isSymbolicLink()) throw new Error("Skill contains an unsupported nested symlink")
    if (entry.isDirectory()) walk(root, name, files, depth + 1)
    else if (entry.isFile()) addFile(root, name, files)
    else throw new Error("Skill contains a special file")
  }
}

function addFile(root, name, files) {
  const file = path.join(root, name)
  const stat = fs.statSync(file)
  if (files.length >= MAX_FILES || stat.size > MAX_BYTES) throw new Error("Skill snapshot too large")
  const data = fs.readFileSync(file)
  files.push({ name, data, mode: stat.mode })
  if (files.reduce((total, item) => total + item.data.length, 0) > MAX_BYTES) {
    throw new Error("Skill snapshot too large")
  }
}

export function fingerprint(files) {
  return digest(JSON.stringify(files.map((item) => [item.name, digest(item.data)])))
}

export function saveSnapshot(directory, files) {
  for (const file of files) {
    const destination = path.join(directory, file.name)
    fs.mkdirSync(path.dirname(destination), { recursive: true })
    fs.writeFileSync(destination, file.data, { mode: file.mode, flag: "wx" })
  }
}

export function withStore(skill, action) {
  const root = fs.realpathSync(skill)
  if (!fs.statSync(path.join(root, "SKILL.md")).isFile()) throw new Error("SKILL.md is required")
  const base = path.join(path.dirname(root), ".iyw-skill-evolution")
  fs.mkdirSync(base, { recursive: true })
  if (fs.lstatSync(base).isSymbolicLink()) throw new Error("Evolution store cannot be a symlink")
  const directory = path.join(base, digest(root).slice(0, 24))
  fs.mkdirSync(directory, { recursive: true })
  if (fs.lstatSync(directory).isSymbolicLink()) throw new Error("Evolution store cannot be a symlink")
  const lock = path.join(directory, "write.lock")
  const handle = fs.openSync(lock, "wx")
  try {
    return action({ root, directory })
  } finally {
    fs.closeSync(handle)
    fs.unlinkSync(lock)
  }
}

export function proposalDirectory(store, id) {
  if (typeof id !== "string" || !/^[a-f0-9]{24}$/.test(id)) throw new Error("Invalid proposal ID")
  const directory = path.join(store.directory, id)
  if (fs.existsSync(directory) && fs.lstatSync(directory).isSymbolicLink()) {
    throw new Error("Proposal cannot be a symlink")
  }
  return directory
}

export function loadProposal(store, id) {
  const directory = proposalDirectory(store, id)
  const record = readJson(path.join(directory, "record.json"))
  if (record.skill !== store.root || record.id !== id) throw new Error("Proposal identity mismatch")
  return { directory, record }
}

export function saveRecord(proposal, event) {
  proposal.record.history.push({ ...event, at: new Date().toISOString() })
  writeJson(path.join(proposal.directory, "record.json"), proposal.record)
}

export function requireCurrent(store, record, expected) {
  if (fingerprint(snapshot(store.root)) !== expected) {
    throw new Error("Skill changed since staging; retain the proposal and stage against the current version")
  }
  targetPath(store.root, record.target)
}

export function replaceTarget(store, record, data) {
  const target = targetPath(store.root, record.target)
  const temporary = `${target}.${crypto.randomUUID()}.tmp`
  try {
    fs.writeFileSync(temporary, data, { flag: "wx", mode: fs.statSync(target).mode })
    fs.renameSync(temporary, target)
  } finally {
    if (fs.existsSync(temporary)) fs.unlinkSync(temporary)
  }
}
