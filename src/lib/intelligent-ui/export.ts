export function downloadUiFile(input: {
  name: string
  content: string
  type: string
}) {
  const url = URL.createObjectURL(
    new Blob([input.content], { type: input.type })
  )
  const link = document.createElement("a")
  link.href = url
  link.download = input.name
  link.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
export function exportUiSource(
  source: string,
  state: Readonly<Record<string, unknown>>
) {
  downloadUiFile({
    name: "intelligent-ui.json",
    content: JSON.stringify({ version: 2, source, state }, null, 2),
    type: "application/json;charset=utf-8",
  })
}
