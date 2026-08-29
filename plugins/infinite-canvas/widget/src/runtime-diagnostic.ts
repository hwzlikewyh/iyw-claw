import type { App } from "@modelcontextprotocol/ext-apps"

export type RuntimeDiagnosticState = {
  canvasId: string
  revision: number | null
  status: "waiting" | "ready" | "error"
  message?: string
}

export function registerRuntimeDiagnostic(app: App, canvasId: string, onChange: (state: RuntimeDiagnosticState) => void): () => void {
  let state: RuntimeDiagnosticState = { canvasId, revision: null, status: "waiting" }
  const publish = (next: RuntimeDiagnosticState) => { state = next; onChange(state) }
  app.ontoolinput = (input) => {
    const incoming = typeof input.arguments?.canvasId === "string" ? input.arguments.canvasId : canvasId
    publish({ canvasId: incoming, revision: state.revision, status: "waiting" })
  }
  app.ontoolresult = (result) => {
    const items = (result.content ?? []) as Array<{ type: string; text?: string }>
    const text = items.find((item) => item.type === "text")
    if (!text || text.type !== "text" || typeof text.text !== "string") return publish({ ...state, status: "error", message: "runtime returned no state" })
    try {
      const value = JSON.parse(text.text) as { canvasId?: string; revision?: number }
      if (typeof value.canvasId !== "string" || typeof value.revision !== "number") throw new Error("invalid state")
      publish({ canvasId: value.canvasId, revision: value.revision, status: "ready" })
    } catch { publish({ ...state, status: "error", message: "runtime state is invalid" }) }
  }
  return () => { app.ontoolinput = undefined; app.ontoolresult = undefined }
}
