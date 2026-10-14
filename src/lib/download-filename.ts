// 保存名称来自成果标题，扩展名来自实际文件，避免标题被当作路径。
export function downloadFilename(name: string, sourceName: string): string {
  const sourceFile = safeFilename(sourceName.split(/[/\\]/).pop() ?? "")
  const filename = safeFilename(name) || sourceFile || "file"
  const extension = sourceFile.match(/[^.]\.([^.]+)$/)?.[1]
  if (
    !extension ||
    filename.toLowerCase().endsWith(`.${extension.toLowerCase()}`)
  ) {
    return filename
  }
  return `${filename}.${extension}`
}

function safeFilename(name: string): string {
  const filename = name
    .replace(/[<>:"/\\|?*\p{Cc}]/gu, "_")
    .trim()
    .replace(/[. ]+$/, "")
  return /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(filename)
    ? `_${filename}`
    : filename
}
