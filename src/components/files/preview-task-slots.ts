const MAX_HEAVY_PREVIEWS = 2
const MAX_QUEUED_PREVIEWS = 16
let active = 0
const waiting: Array<() => void> = []

export function acquirePreviewSlot(signal: AbortSignal): Promise<() => void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(new DOMException("Aborted", "AbortError"))
      return
    }
    if (waiting.length >= MAX_QUEUED_PREVIEWS) {
      reject(new Error("Preview queue is full"))
      return
    }
    const cancel = () => {
      const index = waiting.indexOf(start)
      if (index >= 0) waiting.splice(index, 1)
      reject(new DOMException("Aborted", "AbortError"))
    }
    const start = () => {
      signal.removeEventListener("abort", cancel)
      active += 1
      let released = false
      resolve(() => {
        if (released) return
        released = true
        active -= 1
        waiting.shift()?.()
      })
    }
    if (active < MAX_HEAVY_PREVIEWS) start()
    else {
      waiting.push(start)
      signal.addEventListener("abort", cancel, { once: true })
    }
  })
}
