const HEADER_LENGTH = 48
const MAGIC = [0x49, 0x59, 0x57, 0x42]

export interface BrowserFrame {
  runtimeGeneration: number
  tabGeneration: number
  viewGeneration: number
  seq: number
  width: number
  height: number
  jpeg: Uint8Array
}

export function parseBrowserFrame(
  raw: ArrayBuffer | Uint8Array | number[]
): BrowserFrame {
  const bytes = toBytes(raw)
  if (bytes.byteLength <= HEADER_LENGTH) throw new Error("browser frame empty")
  for (let index = 0; index < MAGIC.length; index += 1) {
    if (bytes[index] !== MAGIC[index]) throw new Error("browser frame magic")
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  if (view.getUint8(4) !== 1 || view.getUint16(6, true) !== HEADER_LENGTH) {
    throw new Error("browser frame version")
  }
  const jpeg = bytes.subarray(HEADER_LENGTH)
  if (
    jpeg[0] !== 0xff ||
    jpeg[1] !== 0xd8 ||
    jpeg[jpeg.length - 2] !== 0xff ||
    jpeg[jpeg.length - 1] !== 0xd9
  ) {
    throw new Error("browser frame jpeg")
  }
  return {
    runtimeGeneration: readSafeU64(view, 8),
    tabGeneration: readSafeU64(view, 16),
    viewGeneration: readSafeU64(view, 24),
    seq: readSafeU64(view, 32),
    width: view.getUint32(40, true),
    height: view.getUint32(44, true),
    jpeg,
  }
}

function toBytes(raw: ArrayBuffer | Uint8Array | number[]): Uint8Array {
  if (raw instanceof Uint8Array) return raw
  if (raw instanceof ArrayBuffer) return new Uint8Array(raw)
  return Uint8Array.from(raw)
}

function readSafeU64(view: DataView, offset: number): number {
  const value = view.getBigUint64(offset, true)
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error("browser frame counter overflow")
  }
  return Number(value)
}
