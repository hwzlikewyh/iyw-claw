import i18n from "@/i18n"
import { useAgentStore } from "@/stores/use-agent-store"
import { flushHostCanvas } from "@/components/layout/plugin-canvas-bridge"
import type {} from "../../../../../widget/src/upstream/host-bridge"

export async function sendHostCanvasTask(prompt: string): Promise<void> {
  const app = window.__IYW_CANVAS_BRIDGE__?.app
  const snapshot = useAgentStore.getState().canvasContext?.snapshot
  if (!app || !snapshot) throw new Error(i18n.t("hostAgent.noCanvas"))
  const scene = await flushHostCanvas(snapshot.projectId)
  const current = useAgentStore.getState().canvasContext?.snapshot
  if (!current || current.projectId !== snapshot.projectId || JSON.stringify(current.selectedNodeIds) !== JSON.stringify(snapshot.selectedNodeIds)) throw new Error(i18n.t("hostAgent.changed"))
  const selected = new Set(snapshot.selectedNodeIds)
  const selectedNodeIds = scene.nodes.filter((node) => selected.has(String((node.metadata as Record<string, unknown> | undefined)?.originalNodeId || node.id))).map((node) => String(node.id))
  const selection = await app.callServerTool({ name: "save_infinite_canvas_selection", arguments: { canvasId: scene.canvasId, revision: scene.revision, selectedNodeIds } })
  if (selection.isError) throw new Error(i18n.t("hostAgent.syncFailed"))
  const context = { canvasId: scene.canvasId, revision: scene.revision, title: snapshot.title, selectedNodeIds }
  const text = `${prompt.trim()}\n\n当前 Infinite Canvas 上下文：${JSON.stringify(context)}\n请通过当前工作区的 Infinite Canvas 能力读取场景，并在此 canvasId 上操作；先读取最新 revision，保留无关节点。修改结果会自动同步到画布。`
  const result = await app.sendMessage({ role: "user", content: [{ type: "text", text }] })
  if (result.isError) throw new Error(i18n.t("hostAgent.rejected"))
}
