import { useEffect, useRef, type RefObject } from "react"
import {
  nativePageCommand,
  type NativePageState,
  type PageBounds,
} from "@/lib/native-page-api"
import { isDesktop } from "@/lib/platform"

type Options = {
  id: string
  url: string
  visible: boolean
  revision: number
  onState: (state: NativePageState) => void
  onError: (message: string) => void
}
const SURFACE_POLL_MS = 250
const STATE_POLL_MS = 1000
const queues = new Map<string, Promise<unknown>>()
function enqueue(id: string, operation: () => Promise<unknown>) {
  const next = (queues.get(id) ?? Promise.resolve()).then(operation, operation)
  queues.set(id, next)
  void next
    .finally(() => {
      if (queues.get(id) === next) queues.delete(id)
    })
    .catch(() => {})
  return next
}
export function useNativePageSurface(
  container: RefObject<HTMLDivElement | null>,
  options: Options
) {
  const latest = useRef(options)
  useEffect(() => {
    latest.current = options
  })
  useEffect(() => {
    if (!isDesktop() || !container.current) return
    const session = new SurfaceSession(container.current, latest)
    const sync = () => void session.sync()
    let frame: number | null = null
    const schedule = () => {
      if (frame !== null) return
      frame = requestAnimationFrame(() => {
        frame = null
        sync()
      })
    }
    const observer = new ResizeObserver(schedule)
    const mutations = new MutationObserver(schedule)
    observer.observe(container.current)
    mutations.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["data-state", "inert", "class"],
    })
    document.addEventListener("visibilitychange", sync)
    const timer = window.setInterval(schedule, SURFACE_POLL_MS)
    sync()
    return () => {
      if (frame !== null) cancelAnimationFrame(frame)
      clearInterval(timer)
      observer.disconnect()
      mutations.disconnect()
      document.removeEventListener("visibilitychange", sync)
      session.close()
    }
  }, [container, options.id, options.url, options.revision])
}
class SurfaceSession {
  private disposed = false
  private ready = false
  private pending = false
  private failed = false
  private last = ""
  private snapshot = ""
  private stateAt = 0
  private initial: Options
  constructor(
    private element: HTMLDivElement,
    private latest: { current: Options }
  ) {
    this.initial = latest.current
  }
  async sync() {
    if (this.disposed || this.pending || this.failed) return
    const rect = this.element.getBoundingClientRect()
    const bounds = {
      x: rect.left,
      y: rect.top,
      width: rect.width,
      height: rect.height,
    }
    const visible = this.visible() && bounds.width > 0 && bounds.height > 0
    const key = visible ? JSON.stringify(bounds) : "hidden"
    if (!this.ready && !visible) return
    if (
      key === this.last &&
      (!visible || performance.now() - this.stateAt < STATE_POLL_MS)
    )
      return
    this.pending = true
    try {
      await enqueue(this.initial.id, () =>
        this.update(bounds, visible, key !== this.last)
      )
      this.last = key
      this.stateAt = performance.now()
    } catch (error) {
      this.failed = true
      if (!this.disposed) this.latest.current.onError(String(error))
    } finally {
      this.pending = false
    }
  }
  private visible() {
    return (
      this.latest.current.visible &&
      document.visibilityState === "visible" &&
      !this.element.closest("[inert]") &&
      !document.querySelector(
        '[data-state="open"][role="dialog"], [data-state="open"][role="menu"], [data-radix-popper-content-wrapper]'
      )
    )
  }
  private async update(bounds: PageBounds, visible: boolean, changed: boolean) {
    if (this.disposed) return
    const id = this.initial.id
    if (!this.ready) {
      const state = await nativePageCommand(id, "create", {
        url: this.initial.url,
        bounds,
      })
      this.ready = true
      this.publish(state)
    }
    if (this.disposed) return
    if (changed) {
      if (visible) await nativePageCommand(id, "bounds", { bounds })
      await nativePageCommand(id, "visible", { visible })
    }
    if (visible && !this.disposed) {
      this.publish(await nativePageCommand(id, "state"))
    }
  }
  private publish(state: NativePageState) {
    const snapshot = JSON.stringify(state)
    if (!this.disposed && this.snapshot !== snapshot) {
      this.snapshot = snapshot
      this.latest.current.onState(state)
    }
  }
  close() {
    this.disposed = true
    void enqueue(this.initial.id, () =>
      nativePageCommand(this.initial.id, "close")
    ).catch(() => {})
  }
}
