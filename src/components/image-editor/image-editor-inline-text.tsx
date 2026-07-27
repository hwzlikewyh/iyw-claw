"use client"

import { useEffect, useRef, type KeyboardEvent } from "react"
import type { StageSize } from "./image-editor-model"
import type { TextDraft } from "./use-image-editor-drawing"

interface ImageEditorInlineTextProps {
  draft: TextDraft
  color: string
  size: StageSize
  displayScale: number
  onChange: (value: string) => void
  onCommit: () => void
  onCancel: () => void
}

const FONT_SIZE = 24
const INPUT_MIN_WIDTH = 96
const INPUT_CHAR_WIDTH = 15
const INPUT_HORIZONTAL_PADDING = 12
const INPUT_HEIGHT = 38
// Minimum pixel sizes so the input stays visible when displayScale is small
const MIN_INPUT_HEIGHT_PX = 28
const MIN_FONT_SIZE_PX = 14
const MIN_INPUT_WIDTH_PX = 80

export function ImageEditorInlineText(props: ImageEditorInlineTextProps) {
  const inputRef = useRef<HTMLInputElement>(null)
  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  // Coordinates from getPointerPosition() are in the stage's internal
  // (unscaled) space. Convert to wrapper-div space by multiplying by
  // displayScale so the input stays inside the overflow-hidden dialog even
  // when the canvas is CSS-scaled down.
  const scale = props.displayScale
  const scaledWidth = props.size.width * scale
  const scaledHeight = props.size.height * scale

  const minWidth = Math.min(
    Math.max(MIN_INPUT_WIDTH_PX, INPUT_MIN_WIDTH * scale),
    scaledWidth
  )
  const inputHeight = Math.min(
    Math.max(MIN_INPUT_HEIGHT_PX, INPUT_HEIGHT * scale),
    scaledHeight
  )
  const left = Math.max(0, Math.min(props.draft.x * scale, scaledWidth - minWidth))
  const top = Math.max(0, Math.min(props.draft.y * scale, scaledHeight - inputHeight))
  const availableWidth = Math.max(1, scaledWidth - left)
  const contentWidth =
    Math.max(1, props.draft.value.length) * INPUT_CHAR_WIDTH * scale +
    INPUT_HORIZONTAL_PADDING * scale
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
        fontSize: Math.max(MIN_FONT_SIZE_PX, FONT_SIZE * scale),
        letterSpacing: 0,
      }}
    />
  )
}
