import type { App } from "@modelcontextprotocol/ext-apps"

export type MigrationSummary = { pageId: string; targetCanvasId: string; mapped: number; skipped: number; warnings: string[] }

export async function previewCowartMigration(app: App, pageId: string): Promise<MigrationSummary> {
  const result = await app.callServerTool({ name: "migrate_cowart_canvas", arguments: { pageId, dryRun: true } })
  if (result.isError) throw new Error("Cowart migration preview failed")
  const text = (result.content as Array<{ type: string; text?: string }>).find((item) => item.type === "text")?.text
  if (!text) throw new Error("Cowart migration preview is empty")
  const value = JSON.parse(text) as { targetCanvasId: string; mapped: number; skipped: number; warnings: string[] }
  return { pageId, targetCanvasId: value.targetCanvasId, mapped: value.mapped, skipped: value.skipped, warnings: value.warnings }
}

export async function confirmCowartMigration(app: App, pageId: string, targetCanvasId?: string): Promise<MigrationSummary & { reportPath?: string }> {
  const result = await app.callServerTool({ name: "migrate_cowart_canvas", arguments: { pageId, ...(targetCanvasId ? { targetCanvasId } : {}) } })
  if (result.isError) throw new Error("Cowart migration failed")
  const text = (result.content as Array<{ type: string; text?: string }>).find((item) => item.type === "text")?.text
  if (!text) throw new Error("Cowart migration result is empty")
  return JSON.parse(text) as MigrationSummary & { reportPath?: string }
}
