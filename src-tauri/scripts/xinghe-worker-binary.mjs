const MACHINE = {
  "x86_64-pc-windows-msvc": ["pe", 0x8664],
  "i686-pc-windows-msvc": ["pe", 0x014c],
  "aarch64-pc-windows-msvc": ["pe", 0xaa64],
  "x86_64-unknown-linux-gnu": ["elf", 62],
  "aarch64-unknown-linux-gnu": ["elf", 183],
  "x86_64-apple-darwin": ["mach", 0x01000007],
  "aarch64-apple-darwin": ["mach", 0x0100000c],
}

const EXPORTS = [
  "iyw_xinghe_worker_run_v1",
  "iyw_xinghe_worker_dispatch_helper_v1",
  "iyw_xinghe_worker_abi_version",
  "iyw_xinghe_worker_core_version",
]

export const WINDOWS_HELPERS = [
  "xinghe-windows-sandbox-setup.exe",
  "xinghe-command-runner.exe",
]

export function helperNames(target) {
  return target.includes("windows")
    ? ["iyw-xinghe-helper.exe", ...WINDOWS_HELPERS]
    : ["iyw-xinghe-helper"]
}

export function verifyHelperBinary(bytes, target) {
  const expected = MACHINE[target]
  if (!expected || bytes.length < 64)
    throw new Error("unsupported or truncated helper")
  const [format, machine] = expected
  if (format === "pe") return verifyWindowsHelper(bytes, target)
  if (
    format === "elf" &&
    bytes.subarray(0, 4).toString("hex") === "7f454c46" &&
    bytes[4] === 2 &&
    bytes[5] === 1 &&
    [2, 3].includes(bytes.readUInt16LE(16)) &&
    bytes.readUInt16LE(18) === machine
  )
    return
  if (
    format === "mach" &&
    bytes.readUInt32LE(0) === 0xfeedfacf &&
    bytes.readUInt32LE(4) === machine &&
    bytes.readUInt32LE(12) === 2
  )
    return
  throw new Error("helper architecture or executable format mismatch")
}

export function verifyWindowsHelper(bytes, target) {
  const expected = MACHINE[target]
  if (!expected || expected[0] !== "pe" || bytes.length < 64)
    throw new Error("invalid Windows sandbox helper target or binary")
  const pe = bytes.readUInt32LE(0x3c)
  if (
    pe + 24 > bytes.length ||
    bytes.toString("ascii", 0, 2) !== "MZ" ||
    bytes.readUInt32LE(pe) !== 0x4550 ||
    bytes.readUInt16LE(pe + 4) !== expected[1] ||
    (bytes.readUInt16LE(pe + 22) & 0x2002) !== 0x0002
  )
    throw new Error(
      "Windows sandbox helper is not an executable for the expected target"
    )
  const optional = pe + 24
  const optionalSize = bytes.readUInt16LE(pe + 20)
  const subsystemOffset = 68
  const windowsGui = 2
  const expectedMagic = expected[1] === 0x014c ? 0x10b : 0x20b
  if (
    optionalSize < subsystemOffset + 2 ||
    optional + optionalSize > bytes.length ||
    bytes.readUInt16LE(optional) !== expectedMagic
  )
    throw new Error("invalid Windows helper optional header")
  if (bytes.readUInt16LE(optional + subsystemOffset) !== windowsGui)
    throw new Error("Windows helper must use the non-console subsystem")
}

export function verifyWorkerBinary(bytes, target, pin) {
  const expected = MACHINE[target]
  if (!expected || bytes.length < 64)
    throw new Error("unsupported or truncated worker binary")
  const [format, machine] = expected
  if (format === "pe") verifyPe(bytes, machine)
  if (
    format === "elf" &&
    (bytes.subarray(0, 4).toString("hex") !== "7f454c46" ||
      bytes[4] !== 2 ||
      bytes[5] !== 1 ||
      bytes.readUInt16LE(16) !== 3 ||
      bytes.readUInt16LE(18) !== machine)
  ) {
    throw new Error("worker ELF architecture mismatch")
  }
  if (
    format === "mach" &&
    (bytes.readUInt32LE(0) !== 0xfeedfacf ||
      bytes.readUInt32LE(4) !== machine ||
      bytes.readUInt32LE(12) !== 6)
  ) {
    throw new Error("worker Mach-O architecture mismatch")
  }
  const identity = `IYW_XINGHE_WORKER|1|${pin.ref.replace(/^rust-v/, "")}|${pin.commit}|END_WORKER_ID\0`
  if (!bytes.includes(Buffer.from(identity)))
    throw new Error("worker binary contains the wrong upstream identity")
  for (const entry of EXPORTS) {
    if (!bytes.includes(Buffer.from(`${entry}\0`)))
      throw new Error(`worker binary lacks ABI symbol ${entry}`)
  }
}

function verifyPe(bytes, machine) {
  const pe = bytes.readUInt32LE(0x3c)
  if (
    bytes.toString("ascii", 0, 2) !== "MZ" ||
    pe + 24 > bytes.length ||
    bytes.readUInt32LE(pe) !== 0x4550
  ) {
    throw new Error("invalid worker PE header")
  }
  if (
    bytes.readUInt16LE(pe + 4) !== machine ||
    (bytes.readUInt16LE(pe + 22) & 0x2000) === 0
  ) {
    throw new Error("worker PE is not a library for the expected architecture")
  }
  const optional = pe + 24
  const magic = bytes.readUInt16LE(optional)
  const directories =
    optional + (magic === 0x20b ? 112 : magic === 0x10b ? 96 : 0)
  if (directories === optional || directories + 8 > bytes.length)
    throw new Error("invalid worker PE optional header")
  const table = pe + 24 + bytes.readUInt16LE(pe + 20)
  const sections = bytes.readUInt16LE(pe + 6)
  const atRva = (rva, size = 1) => peOffset(bytes, table, sections, rva, size)
  const directory = atRva(bytes.readUInt32LE(directories), 40)
  const count = bytes.readUInt32LE(directory + 24)
  const names = atRva(bytes.readUInt32LE(directory + 32), count * 4)
  const found = new Set()
  for (let index = 0; index < count; index++) {
    const start = atRva(bytes.readUInt32LE(names + index * 4))
    const end = bytes.indexOf(0, start)
    if (end < 0 || end - start > 1024)
      throw new Error("invalid worker PE export name")
    found.add(bytes.toString("ascii", start, end).replace(/^_/, ""))
  }
  for (const name of EXPORTS) {
    if (!found.has(name)) throw new Error(`worker PE does not export ${name}`)
  }
}

export function windowsRuntimeImports(bytes, target) {
  const DIRECTORY_BYTES = 8
  const IMPORT_BYTES = 20
  const MAX_DLL_NAME = 256
  if (bytes.length < 64 || bytes.toString("ascii", 0, 2) !== "MZ")
    throw new Error("invalid Windows runtime DOS header")
  const pe = bytes.readUInt32LE(0x3c)
  if (
    pe + 24 > bytes.length ||
    bytes.readUInt32LE(pe) !== 0x4550 ||
    bytes.readUInt16LE(pe + 4) !== MACHINE[target]?.[1]
  )
    throw new Error("invalid Windows runtime architecture")
  const optional = pe + 24
  if (optional + 2 > bytes.length)
    throw new Error("truncated Windows runtime optional header")
  const magic = bytes.readUInt16LE(optional)
  const directories =
    optional + (magic === 0x20b ? 112 : magic === 0x10b ? 96 : 0)
  const optionalEnd = optional + bytes.readUInt16LE(pe + 20)
  if (
    directories === optional ||
    directories + 2 * DIRECTORY_BYTES > optionalEnd ||
    optionalEnd > bytes.length
  )
    throw new Error("invalid Windows runtime import header")
  const rva = bytes.readUInt32LE(directories + DIRECTORY_BYTES)
  const size = bytes.readUInt32LE(directories + DIRECTORY_BYTES + 4)
  if (rva === 0 && size === 0) return []
  const sections = bytes.readUInt16LE(pe + 6)
  const atRva = (address, length = 1) =>
    peOffset(bytes, optionalEnd, sections, address, length)
  const table = atRva(rva, size)
  const imports = []
  for (let offset = 0; offset + IMPORT_BYTES <= size; offset += IMPORT_BYTES) {
    const nameRva = bytes.readUInt32LE(table + offset + 12)
    if (nameRva === 0) return imports
    const start = atRva(nameRva)
    const end = bytes.indexOf(0, start)
    if (end < 0 || end - start > MAX_DLL_NAME)
      throw new Error("invalid Windows runtime import name")
    const name = bytes.toString("ascii", start, end)
    if (/^(?:vcruntime|msvcp|concrt|vcomp)\d[^\\/]*\.dll$/i.test(name))
      imports.push(name.toLowerCase())
  }
  throw new Error("unterminated Windows runtime import table")
}

function peOffset(bytes, table, count, rva, size) {
  const SECTION_BYTES = 40
  if (table + count * SECTION_BYTES > bytes.length)
    throw new Error("invalid worker PE sections")
  for (let index = 0; index < count; index++) {
    const section = table + index * SECTION_BYTES
    const address = bytes.readUInt32LE(section + 12)
    const length = bytes.readUInt32LE(section + 16)
    const offset = bytes.readUInt32LE(section + 20) + rva - address
    if (
      rva >= address &&
      rva + size <= address + length &&
      offset + size <= bytes.length
    )
      return offset
  }
  throw new Error("worker PE export address is outside its sections")
}
