"use client"

import { useEffect, useRef, useState, type RefObject } from "react"
import { browserApi } from "@/lib/browser-api"
import type { BrowserScreenshot } from "@/lib/browser-types"

const VISIBILITY_POLL_MS = 250
const PAINT_TIMEOUT_MS = 100
const queues = new Map<string, Promise<unknown>>()
const OVERLAYS =
  '[role="dialog"], [role="alertdialog"], [role="menu"], [data-slot="popover-content"], [data-slot="dropdown-menu-content"]'

interface Options {
  container: RefObject<HTMLDivElement | null>
  tabId: string
  selecting: boolean
  onSelectionFrame?: (frame: BrowserScreenshot | null) => void
}

interface Placement {
  bounds: { x: number; y: number; width: number; height: number }
  onScreen: boolean
  freeze: boolean
  visible: boolean
  key: string
}

export function useNativeBrowserPlacement(options: Options) {
  const { container, tabId, selecting, onSelectionFrame } = options
  const [frame, setFrame] = useState<BrowserScreenshot | null>(null)
  const [error, setError] = useState<string | null>(null)
  const intent = useRef({ selecting, onSelectionFrame })
  const requestSync = useRef<() => void>(() => {})
  useEffect(() => {
    intent.current = { selecting, onSelectionFrame }
    requestSync.current()
  }, [selecting, onSelectionFrame])
  useEffect(() => {
    const element = container.current
    if (!element) return
    const runtime = new NativeBrowserPlacement({
      element,
      tabId,
      intent: () => intent.current,
      setFrame,
      setError,
    })
    requestSync.current = runtime.sync
    const stopObserving = observeSurface(element, runtime.sync)
    runtime.sync()
    return () => {
      requestSync.current = () => {}
      stopObserving()
      runtime.dispose()
    }
  }, [container, tabId])
  return { frame, error }
}

interface RuntimeOptions {
  element: HTMLDivElement
  tabId: string
  intent: () => Pick<Options, "selecting" | "onSelectionFrame">
  setFrame: (frame: BrowserScreenshot | null) => void
  setError: (error: string | null) => void
}

class NativeBrowserPlacement {
  private disposed = false
  private pending = false
  private dirty = false
  private last = ""
  private frozen: BrowserScreenshot | null = null

  constructor(private readonly options: RuntimeOptions) {}

  private desired() {
    return measurePlacement(
      this.options.element,
      this.options.intent().selecting
    )
  }

  private paint(frame: BrowserScreenshot | null) {
    this.frozen = frame
    this.options.setFrame(frame)
  }

  private async freeze() {
    try {
      const frame = await browserApi.nativePreview(this.options.tabId)
      const image = new Image()
      image.src = `data:${frame.mimeType};base64,${frame.data}`
      await image.decode()
      if (this.disposed || !this.desired().freeze) return
      this.paint(frame)
      await nextPaint()
    } catch (cause) {
      if (!this.disposed)
        console.warn("[Browser] overlay preview capture failed", {
          code: (cause as { code?: string })?.code ?? "capture_failed",
        })
    }
  }
  private async apply() {
    let placement = this.desired()
    if (placement.key !== this.last) {
      // 先绘制替代画面再隐藏 WebView；菜单关闭后重新测量，取消过期的隐藏请求。
      if (placement.freeze && this.last !== "hidden") await this.freeze()
      if (this.disposed) return
      placement = this.desired()
      await browserApi.nativeView({
        tabId: this.options.tabId,
        visible: placement.visible,
        ...(placement.visible ? { bounds: placement.bounds } : {}),
      })
      if (this.disposed) return
      this.last = placement.key
      if (placement.visible || !placement.onScreen) this.paint(null)
      this.options.setError(null)
    }
    this.options
      .intent()
      .onSelectionFrame?.(this.options.intent().selecting ? this.frozen : null)
  }

  sync = () => {
    if (this.disposed) return
    this.dirty = true
    if (this.pending) return
    this.pending = true
    this.dirty = false
    void enqueue(this.options.tabId, async () => {
      if (!this.disposed) await this.apply()
    })
      .catch((cause) => {
        if (!this.disposed)
          this.options.setError(
            String((cause as { message?: string })?.message ?? cause)
          )
      })
      .finally(() => {
        this.pending = false
        if (this.dirty && !this.disposed) this.sync()
      })
  }

  dispose() {
    this.disposed = true
    this.options.intent().onSelectionFrame?.(null)
    void enqueue(this.options.tabId, () =>
      browserApi.nativeView({ tabId: this.options.tabId, visible: false })
    ).catch(() => {})
  }
}

function measurePlacement(
  element: HTMLDivElement,
  selecting: boolean
): Placement {
  const rect = element.getBoundingClientRect()
  const bounds = {
    x: rect.left,
    y: rect.top,
    width: rect.width,
    height: rect.height,
  }
  const onScreen =
    document.visibilityState === "visible" &&
    element.getClientRects().length > 0 &&
    rect.width > 0 &&
    rect.height > 0 &&
    getComputedStyle(element).visibility !== "hidden"
  const overlay =
    element.closest('[inert], [aria-hidden="true"]') !== null ||
    Array.from(document.querySelectorAll<HTMLElement>(OVERLAYS)).some(
      (item) =>
        item.getClientRects().length > 0 &&
        getComputedStyle(item).visibility !== "hidden" &&
        item.getAttribute("aria-hidden") !== "true"
    )
  const freeze = onScreen && (selecting || overlay)
  const visible = onScreen && !freeze
  return {
    bounds,
    onScreen,
    freeze,
    visible,
    key: visible ? JSON.stringify(bounds) : "hidden",
  }
}

function observeSurface(element: HTMLDivElement, sync: () => void) {
  const resize = new ResizeObserver(sync)
  const mutations = new MutationObserver(sync)
  resize.observe(element)
  mutations.observe(document.body, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["data-state", "inert", "aria-hidden", "class", "style"],
  })
  document.addEventListener("visibilitychange", sync)
  const timer = window.setInterval(sync, VISIBILITY_POLL_MS)
  return () => {
    resize.disconnect()
    mutations.disconnect()
    document.removeEventListener("visibilitychange", sync)
    clearInterval(timer)
  }
}

function enqueue(tabId: string, operation: () => Promise<unknown>) {
  const next = (queues.get(tabId) ?? Promise.resolve()).then(
    operation,
    operation
  )
  queues.set(tabId, next)
  void next
    .finally(() => {
      if (queues.get(tabId) === next) queues.delete(tabId)
    })
    .catch(() => {})
  return next
}

function nextPaint(): Promise<void> {
  return new Promise((resolve) => {
    let first = 0
    let second = 0
    const finish = () => {
      clearTimeout(timer)
      cancelAnimationFrame(first)
      cancelAnimationFrame(second)
      resolve()
    }
    const timer = window.setTimeout(finish, PAINT_TIMEOUT_MS)
    first = requestAnimationFrame(() => {
      second = requestAnimationFrame(finish)
    })
  })
}
