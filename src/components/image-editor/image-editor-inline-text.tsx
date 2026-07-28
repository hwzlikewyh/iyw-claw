"use client"

import { useCallback, useEffect, useRef, type KeyboardEvent } from "react"
import { useTranslations } from "next-intl"
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
// How long after mount a blur is still attributed to the browser stealing focus
// rather than to the user clicking away on purpose.
const FOCUS_RACE_WINDOW_MS = 350

export function ImageEditorInlineText(props: ImageEditorInlineTextProps) {
  const t = useTranslations("Folder.chat.messageList")
  const inputRef = useRef<HTMLInputElement>(null)
  // The draft is created from a Konva mousedown listener, which lives outside
  // React's event system. The browser's own mousedown default action moves
  // focus to the nearest focusable ancestor (the dialog panel), which blurs the
  // freshly mounted input and discards it through onBlur before the user can
  // type anything. The stage handler preventDefault()s to stop that, but touch
  // and non-cancelable events can still slip through, so a blur landing in the
  // first moments after mount reclaims focus instead of committing nothing.
  const mountedAtRef = useRef(0)
  const reclaimedRef = useRef(false)
  const focus = useCallback(() => {
    const input = inputRef.current
    if (!input || document.activeElement === input) return
    input.focus({ preventScroll: true })
  }, [])
  useEffect(() => {
    mountedAtRef.current = Date.now()
    focus()
    // A second attempt after the browser has finished its mousedown default
    // action covers the case where the first focus() was undone.
    const frame = requestAnimationFrame(focus)
    return () => cancelAnimationFrame(frame)
  }, [focus])

  const handleBlur = () => {
    // Only the immediate post-mount blur is treated as the focus race — a later
    // click away is a deliberate dismissal and must still close the input.
    const stray =
      !props.draft.value &&
      !reclaimedRef.current &&
      Date.now() - mountedAtRef.current < FOCUS_RACE_WINDOW_MS
    if (stray) {
      reclaimedRef.current = true
      requestAnimationFrame(focus)
      return
    }
    props.onCommit()
  }

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
  const left = Math.max(
    0,
    Math.min(props.draft.x * scale, scaledWidth - minWidth)
  )
  const top = Math.max(
    0,
    Math.min(props.draft.y * scale, scaledHeight - inputHeight)
  )
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
      autoFocus
      placeholder={t("imageEditorTextPlaceholder")}
      aria-label={t("imageEditorTextPlaceholder")}
      onChange={(event) => props.onChange(event.target.value)}
      onKeyDown={handleKeyDown}
      onBlur={handleBlur}
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
