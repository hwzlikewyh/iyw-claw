"use client"

import { useMemo } from "react"

const MAX_HIGHLIGHT_CHARS = 50_000
const MAX_HIGHLIGHT_LINES = 1_000

export function shouldHighlightCode(code: string): boolean {
  if (code.length > MAX_HIGHLIGHT_CHARS) return false
  let lines = 1
  for (let index = 0; index < code.length; index += 1) {
    if (code[index] === "\n" && ++lines > MAX_HIGHLIGHT_LINES) return false
  }
  return true
}

export function PlainCodeBlock({
  code,
  showLineNumbers,
}: {
  code: string
  showLineNumbers: boolean
}) {
  const numbers = useMemo(
    () =>
      showLineNumbers
        ? Array.from(
            { length: code.split("\n").length },
            (_, index) => index + 1
          ).join("\n")
        : null,
    [code, showLineNumbers]
  )
  // 大段输出保留全文，以文本节点显示，避免高亮 token 和逐词 DOM 膨胀。
  return (
    <pre className="m-0 flex overflow-auto p-4 font-mono text-sm">
      {numbers && (
        <code
          aria-hidden="true"
          className="me-4 shrink-0 select-none text-right text-muted-foreground/50"
        >
          {numbers}
        </code>
      )}
      <code className="block">{code}</code>
    </pre>
  )
}
