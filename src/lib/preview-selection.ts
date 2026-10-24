export function selectedPreviewText(root: HTMLElement | null): string {
  const selection = window.getSelection()
  if (!root || !selection?.rangeCount) return ""
  const range = selection.getRangeAt(0)
  if (
    !root.contains(range.startContainer) ||
    !root.contains(range.endContainer)
  )
    return ""
  const source = root.querySelector<HTMLTextAreaElement>(
    "[data-artifact-preview-source]"
  )
  const content = root.querySelector("[data-artifact-preview-text]")
  if (source && content) {
    const full = document.createRange()
    full.selectNodeContents(content)
    if (
      range.compareBoundaryPoints(Range.START_TO_START, full) <= 0 &&
      range.compareBoundaryPoints(Range.END_TO_END, full) >= 0
    )
      return source.value
  }
  return selection.toString()
}
