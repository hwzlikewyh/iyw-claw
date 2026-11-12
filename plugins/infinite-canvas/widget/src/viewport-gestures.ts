import type { Scene } from "./scene-renderer.js"
import type { CanvasClient } from "./canvas-client.js"

const SAVE_DELAY_MS = 180
type Options = {
  surface: HTMLElement; sceneElement: HTMLElement; client: CanvasClient
  getScene: () => Scene; setScene: (scene: Scene) => void
  markMutation: () => void; render: () => void; report: (error: unknown) => void
}
type Pan = { pointerId: number; startX: number; startY: number; originX: number; originY: number; hadPendingSave: boolean }

export function bindViewportGestures(options: Options) {
  return new ViewportGestures(options)
}

class ViewportGestures {
  private pan: Pan | undefined
  private timer: ReturnType<typeof setTimeout> | undefined
  private generation = 0
  private active = 0
  constructor(private readonly options: Options) {
    const surface = options.surface
    surface.addEventListener("wheel", (event) => this.wheel(event), { passive: false })
    surface.addEventListener("pointerdown", (event) => this.beginPan(event))
    surface.addEventListener("pointermove", (event) => this.movePan(event))
    surface.addEventListener("pointerup", (event) => this.finishPan(event))
    surface.addEventListener("pointercancel", (event) => this.finishPan(event))
  }
  pending() { return Boolean(this.pan || this.timer || this.active) }
  reset() {
    this.generation += 1
    if (this.pan && this.options.surface.hasPointerCapture(this.pan.pointerId)) this.options.surface.releasePointerCapture(this.pan.pointerId)
    this.pan = undefined
    if (this.timer) clearTimeout(this.timer)
    this.timer = undefined
  }
  dispose() { this.reset() }
  private paint() {
    const v = this.options.getScene().viewport
    this.options.sceneElement.style.transform = `translate(${v.x}px, ${v.y}px) scale(${v.k})`
  }
  private schedule() {
    if (this.timer) clearTimeout(this.timer)
    this.timer = setTimeout(() => { void this.save() }, SAVE_DELAY_MS)
  }
  private async save() {
    this.timer = undefined
    const scene = this.options.getScene(), canvasId = scene.canvasId, epoch = this.generation
    this.active += 1
    try {
      const next = await this.options.client.setViewport(scene.viewport)
      if (epoch !== this.generation || this.options.getScene().canvasId !== canvasId) return
      const current = this.options.getScene()
      if (current.revision > next.revision) return
      this.options.setScene({ ...next, viewport: current.viewport } as Scene)
      this.options.render()
    } catch (error) { if (epoch === this.generation) this.options.report(error) }
    finally { this.active -= 1 }
  }
  private wheel(event: WheelEvent) {
    if ((event.target as HTMLElement).closest(".node") || !event.deltaY) return
    event.preventDefault()
    const scene = this.options.getScene()
    const k = Math.max(0.05, Math.min(5, scene.viewport.k * (event.deltaY > 0 ? 0.9 : 1.1)))
    if (k === scene.viewport.k) return
    scene.viewport = { ...scene.viewport, k }
    this.options.markMutation(); this.paint(); this.schedule()
  }
  private beginPan(event: PointerEvent) {
    if ((event.target as HTMLElement).closest(".node") || event.button !== 0) return
    const v = this.options.getScene().viewport
    const hadPendingSave = Boolean(this.timer)
    if (this.timer) clearTimeout(this.timer)
    this.timer = undefined
    this.pan = { pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, originX: v.x, originY: v.y, hadPendingSave }
    this.options.surface.setPointerCapture(event.pointerId)
    event.preventDefault()
  }
  private movePan(event: PointerEvent) {
    if (!this.pan || event.pointerId !== this.pan.pointerId) return
    const scene = this.options.getScene()
    scene.viewport = { ...scene.viewport, x: this.pan.originX + event.clientX - this.pan.startX, y: this.pan.originY + event.clientY - this.pan.startY }
    this.options.markMutation(); this.paint()
  }
  private finishPan(event: PointerEvent) {
    if (!this.pan || event.pointerId !== this.pan.pointerId) return
    const viewport = this.options.getScene().viewport
    const moved = viewport.x !== this.pan.originX || viewport.y !== this.pan.originY || this.pan.hadPendingSave
    this.pan = undefined
    if (this.options.surface.hasPointerCapture(event.pointerId)) this.options.surface.releasePointerCapture(event.pointerId)
    if (moved) this.schedule()
  }
}
