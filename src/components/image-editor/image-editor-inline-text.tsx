"use client"

import { useEffect, useRef, type KeyboardEvent } from "react"
import type { StageSize } from "./image-editor-model"
import type { TextDraft } from "./use-image-editor-drawing"

interface ImageEditorInlineTextProps {
  draft: TextDraft
  color: string
  size: StageSize
  onChange: (value: string) => void
  onCommit: () => void
  onCancel: () => void
}

const FONT_SIZE = 24
const INPUT_MIN_WIDTH = 96
const INPUT_CHAR_WIDTH = 15
const INPUT_HORIZONTAL_PADDING = 12
const INPUT_HEIGHT = 38

export function ImageEditorInlineText(props: ImageEditorInlineTextProps) {
  const inputRef = useRef<HTMLInputElement>(null)
  useEffect(() => {
    inputRef.current?.focus()
  }, [])
  const minWidth = Math.min(INPUT_MIN_WIDTH, props.size.width)
  const inputHeight = Math.min(INPUT_HEIGHT, props.size.height)
  const left = Math.max(0, Math.min(props.draft.x, props.size.width - minWidth))
  const top = Math.max(
    0,
    Math.min(props.draft.y, props.size.height - inputHeight)
  )
  const availableWidth = Math.max(1, props.size.width - left)
  const contentWidth =
    Math.max(1, props.draft.value.length) * INPUT_CHAR_WIDTH +
    INPUT_HORIZONTAL_PADDING
  const width = Math.min(availableWidth, Math.max(minWidth, contentWidth))
  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      event.preventDefault()
      event.stopPropagation()
      props.onCancel()
      return
    }
    if (
      event.key === "Enter" &&
      !event.nativeEvent.isComposing &&
      event.keyCode !== 229
    ) {
      event.preventDefault()
      event.stopPropagation()
      props.onCommit()
    }
  }
  return (
    <input
      ref={inputRef}
      value={props.draft.value}
      aria-label="Annotation text"
      onChange={(event) => props.onChange(event.target.value)}
      onKeyDown={handleKeyDown}
      onBlur={props.onCommit}
      onPointerDown={(event) => event.stopPropagation()}
      className="absolute z-10 rounded-sm border border-blue-500 bg-black/75 px-1.5 py-0.5 font-bold outline-none ring-2 ring-blue-500/20"
      style={{
        left,
        top,
        width,
        height: inputHeight,
        color: props.color,
        fontSize: FONT_SIZE,
        letterSpacing: 0,
      }}
    />
  )
}
