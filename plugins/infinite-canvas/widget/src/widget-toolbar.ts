import type { App } from "@modelcontextprotocol/ext-apps"
import { confirmCowartMigration, previewCowartMigration } from "./migration/cowart-migration-dialog.js"

export function addMigrationAction(toolbar: HTMLElement, app: App, onResult: (value: unknown) => void): void {
  const button = document.createElement("button")
  button.type = "button"
  button.textContent = "Import Cowart"
  button.addEventListener("click", async () => {
    const pageId = window.prompt("Cowart page ID")?.trim()
    if (!pageId) return
    const summary = await previewCowartMigration(app, pageId)
    if (!window.confirm(`Create ${summary.targetCanvasId} without changing the old page?`)) return
    onResult(await confirmCowartMigration(app, pageId, summary.targetCanvasId))
  })
  toolbar.append(button)
}
