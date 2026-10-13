"use client"

import { useEffect, useRef, useState, type PointerEvent } from "react"
import {
  captureRegion,
  drawCaptureMark,
  loadCapture,
  MIN_CAPTURE_DRAG,
  type PageCapture,
  type CaptureMark,
  type CapturePoint,
  type CaptureRegion,
} from "@/lib/page-capture"

type Props = {
  capture: PageCapture
  tool: "crop" | "box" | "arrow"
  marks: CaptureMark[]
  region: CaptureRegion | null
  onMark: (mark: CaptureMark) => void
  onRegion: (region: CaptureRegion) => void
  onError: () => void
  label: string
}

export function PageCaptureCanvas(props: Props) {
  const { capture, onError } = props
  const canvas = useRef<HTMLCanvasElement>(null)
  const [drag, setDrag] = useState<{
    start: CapturePoint
    end: CapturePoint
  } | null>(null)
  const image = useCaptureImage(capture, onError)
  useEffect(() => {
    if (!canvas.current || !image) return
    paintCapture(canvas.current, image, props, drag)
  }, [image, props, drag])
  const finish = (event: PointerEvent<HTMLCanvasElement>) => {
    if (!drag) return
    const end = position(event)
    const region = captureRegion(drag.start, end)
    setDrag(null)
    if (Math.max(region.width, region.height) < MIN_CAPTURE_DRAG) return
    if (props.tool === "crop") {
      if (Math.min(region.width, region.height) >= MIN_CAPTURE_DRAG)
        props.onRegion(region)
    } else props.onMark({ tool: props.tool, start: drag.start, end })
  }
  return (
    <canvas
      ref={canvas}
      aria-label={props.label}
      className="block h-auto w-full touch-none cursor-crosshair"
      onPointerDown={(event) => {
        if (event.button !== 0) return
        event.currentTarget.setPointerCapture(event.pointerId)
        setDrag({ start: position(event), end: position(event) })
      }}
      onPointerMove={(event) => {
        if (drag) setDrag({ ...drag, end: position(event) })
      }}
      onPointerUp={finish}
      onPointerCancel={() => setDrag(null)}
    />
  )
}

function position(event: PointerEvent<HTMLCanvasElement>): CapturePoint {
  const canvas = event.currentTarget
  const rect = canvas.getBoundingClientRect()
  return {
    x: Math.max(
      0,
      Math.min(
        canvas.width,
        ((event.clientX - rect.left) * canvas.width) / rect.width
      )
    ),
    y: Math.max(
      0,
      Math.min(
        canvas.height,
        ((event.clientY - rect.top) * canvas.height) / rect.height
      )
    ),
  }
}

function paintCapture(
  canvas: HTMLCanvasElement,
  image: HTMLImageElement,
  props: Props,
  drag: { start: CapturePoint; end: CapturePoint } | null
) {
  canvas.width = image.naturalWidth
  canvas.height = image.naturalHeight
  const ctx = canvas.getContext("2d")
  if (!ctx) return
  ctx.drawImage(image, 0, 0)
  props.marks.forEach((mark) => drawCaptureMark(ctx, mark))
  if (drag && props.tool !== "crop")
    drawCaptureMark(ctx, { tool: props.tool, ...drag })
  const region =
    props.tool === "crop" && drag
      ? captureRegion(drag.start, drag.end)
      : props.region
  if (!region) return
  ctx.fillStyle = "#00000066"
  ctx.beginPath()
  ctx.rect(0, 0, canvas.width, canvas.height)
  ctx.rect(region.x, region.y, region.width, region.height)
  ctx.fill("evenodd")
  ctx.strokeStyle = "#7c3aed"
  ctx.lineWidth = 2
  ctx.strokeRect(region.x, region.y, region.width, region.height)
}

function useCaptureImage(capture: PageCapture, onError: () => void) {
  const [image, setImage] = useState<HTMLImageElement | null>(null)
  useEffect(() => {
    let active = true
    void loadCapture(capture).then(
      (image) => {
        if (active) setImage(image)
      },
      () => {
        if (active) onError()
      }
    )
    return () => {
      active = false
    }
  }, [capture, onError])
  return image
}
