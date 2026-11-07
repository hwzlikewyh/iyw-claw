const replacements = [
  ["Join us on Discord", "Upstream community"],
  ["PhotoCraft website", "Upstream website  "],
  ["PhotoCraft Website", "Upstream Website  "],
  ["ArtCraft", "Upstream"],
  ["PhotoCraft", "IYW Editor"],
]

// 只改静态data段等长UTF-8文本，保持所有内存地址、长度、函数和许可证文件。
export function replaceBrandText(bytes) {
  const output = Buffer.from(bytes)
  const ranges = dataRanges(output),
    changes = []
  for (const [before, after] of replacements) {
    const input = Buffer.from(before),
      replacement = Buffer.from(after)
    if (input.length !== replacement.length)
      throw Error("Brand text must preserve WASM offsets")
    let position = 0,
      count = 0
    while ((position = output.indexOf(input, position)) >= 0) {
      if (
        !ranges.some(
          ([start, end]) => position >= start && position + input.length <= end
        )
      )
        throw Error("Brand text appears outside static WASM data")
      replacement.copy(output, position)
      position += replacement.length
      count++
    }
    if (!count) throw Error("Upstream brand strings changed")
    changes.push({ before, after, count })
  }
  if (!WebAssembly.validate(output))
    throw Error("Modified editor WASM is invalid")
  return { bytes: output, changes }
}

function dataRanges(bytes) {
  if (!bytes.subarray(0, 8).equals(Buffer.from([0, 97, 115, 109, 1, 0, 0, 0])))
    throw Error("Expected WebAssembly module")
  const cursor = { position: 8 },
    ranges = []
  while (cursor.position < bytes.length) {
    const section = bytes[cursor.position++],
      size = unsigned(bytes, cursor),
      end = cursor.position + size
    if (end > bytes.length) throw Error("Invalid WASM section bounds")
    if (section === 11) {
      const count = unsigned(bytes, cursor)
      for (let index = 0; index < count; index++) {
        const flags = unsigned(bytes, cursor)
        if (flags === 2) unsigned(bytes, cursor)
        if (flags === 0 || flags === 2) {
          const opcode = bytes[cursor.position++]
          if (![0x41, 0x23].includes(opcode))
            throw Error("Unsupported WASM data offset")
          unsigned(bytes, cursor)
          if (bytes[cursor.position++] !== 0x0b)
            throw Error("Unexpected WASM offset expression")
        } else if (flags !== 1) throw Error("Unsupported WASM data segment")
        const length = unsigned(bytes, cursor),
          start = cursor.position
        cursor.position += length
        if (cursor.position > end) throw Error("Invalid WASM data bounds")
        ranges.push([start, cursor.position])
      }
    }
    cursor.position = end
  }
  return ranges
}

function unsigned(bytes, cursor) {
  let value = 0,
    shift = 0,
    byte
  do {
    if (cursor.position >= bytes.length || shift > 28)
      throw Error("Invalid WASM integer")
    byte = bytes[cursor.position++]
    value += (byte & 0x7f) * 2 ** shift
    shift += 7
  } while (byte & 0x80)
  return value
}
