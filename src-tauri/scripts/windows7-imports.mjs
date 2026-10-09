// 与 DLL 检查一起拒绝 Win8/Win10 的静态入口，避免安装成功后在加载器阶段失败。
const PE_POINTER = 0x3c
const COFF_HEADER_BYTES = 24
const OPTIONAL_HEADER64_BYTES = 112
const OPTIONAL_HEADER32_BYTES = 96
const IMPORT_DIRECTORY_OFFSET = 8
const IMPORT_DESCRIPTOR_BYTES = 20
const SECTION_BYTES = 40
const ORDINAL_FLAG64 = 1n << 63n
const MAX_FUNCTION_NAME = 256
const MISSING_WIN7_FUNCTIONS = new Set([
  // x86 系统 DLL 导出未修饰名称，不能导入 Rust 的 stdcall 符号名称。
  "_LoadLibraryExW@12",
  "_GetModuleHandleW@4",
  "_GetProcAddress@8",
  "_SetLastError@4",
  "GetPackagesByPackageFamily",
  "GetCurrentPackageFullName",
  "GetPackageFullName",
  "GetPackageFamilyName",
  "GetStagedPackagePathByFullName",
  "CoIncrementMTAUsage",
  "CreateAppContainerProfile",
  "DeleteAppContainerProfile",
  "DeriveAppContainerSidFromAppContainerName",
  "DeriveCapabilitySidsFromName",
  "GetAppContainerFolderPath",
  "CreatePseudoConsole",
  "ResizePseudoConsole",
  "ClosePseudoConsole",
  "GetSystemTimePreciseAsFileTime",
  "IsWow64Process2",
  "SetThreadDescription",
  "GetThreadDescription",
  "GetDpiForWindow",
  "GetDpiForSystem",
  "ProcessPrng",
])

function offsetFor(bytes, sections, rva) {
  for (let index = 0; index < sections.count; index++) {
    const section = sections.table + index * SECTION_BYTES
    const address = bytes.readUInt32LE(section + 12)
    const length = bytes.readUInt32LE(section + 16)
    if (rva >= address && rva < address + length)
      return bytes.readUInt32LE(section + 20) + rva - address
  }
  throw new Error("Win7 import address is outside PE sections")
}

function importedNames(bytes, sections, thunkRva, thunkBytes, ordinalFlag) {
  const names = []
  const start = offsetFor(bytes, sections, thunkRva)
  for (
    let offset = start;
    offset + thunkBytes <= bytes.length;
    offset += thunkBytes
  ) {
    const thunk = thunkBytes === 8
      ? bytes.readBigUInt64LE(offset)
      : BigInt(bytes.readUInt32LE(offset))
    if (thunk === 0n) return names
    if ((thunk & ordinalFlag) !== 0n) continue
    const name = offsetFor(bytes, sections, Number(thunk)) + 2
    const end = bytes.indexOf(0, name)
    if (end < name || end - name > MAX_FUNCTION_NAME)
      throw new Error("Win7 binary has an invalid import function name")
    names.push(bytes.toString("ascii", name, end))
  }
  throw new Error("Win7 binary has an unterminated import thunk")
}

export function verifyWin7FunctionImports(bytes) {
  const pe = bytes.readUInt32LE(PE_POINTER)
  const optional = pe + COFF_HEADER_BYTES
  const magic = bytes.readUInt16LE(optional)
  const optionalBytes = magic === 0x20b ? OPTIONAL_HEADER64_BYTES :
    magic === 0x10b ? OPTIONAL_HEADER32_BYTES : 0
  if (!optionalBytes) throw new Error("Win7 binary has an invalid PE optional header")
  const sections = {
    table: optional + bytes.readUInt16LE(pe + 20),
    count: bytes.readUInt16LE(pe + 6),
  }
  const directory = optional + optionalBytes + IMPORT_DIRECTORY_OFFSET
  const thunkBytes = magic === 0x20b ? 8 : 4
  const ordinalFlag = magic === 0x20b ? ORDINAL_FLAG64 : 0x80000000n
  const rva = bytes.readUInt32LE(directory)
  if (rva === 0) return
  const table = offsetFor(bytes, sections, rva)
  const names = []
  for (
    let offset = table;
    offset + IMPORT_DESCRIPTOR_BYTES <= bytes.length;
    offset += IMPORT_DESCRIPTOR_BYTES
  ) {
    if (bytes.readUInt32LE(offset + 12) === 0) break
    const thunk = bytes.readUInt32LE(offset) || bytes.readUInt32LE(offset + 16)
    names.push(...importedNames(bytes, sections, thunk, thunkBytes, ordinalFlag))
  }
  const unavailable = names.filter((name) => MISSING_WIN7_FUNCTIONS.has(name))
  if (unavailable.length)
    throw new Error(
      `Win7 binary imports unavailable system functions: ${[...new Set(unavailable)].join(", ")}`
    )
}
