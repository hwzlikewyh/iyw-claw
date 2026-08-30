import type { App } from "@modelcontextprotocol/ext-apps"
import { confirmCowartMigration, discoverCowartPages, previewCowartMigration } from "./migration/cowart-migration-dialog.js"

export function addMigrationAction(toolbar: HTMLElement, app: App, onResult: (value: unknown) => void): void {
  const button = document.createElement("button")
  button.type = "button"
  button.textContent = "Import Cowart"
  button.addEventListener("click", () => { void runMigration(app, onResult).catch((error) => window.alert(error instanceof Error ? error.message : "Cowart migration failed")) })
  toolbar.append(button)
}

async function runMigration(app: App, onResult: (value: unknown) => void): Promise<void> {
  const pages = await discoverCowartPages(app)
  if (!pages.length) { window.alert("No Cowart pages found"); return }
  const pageId = choosePage(pages)
  if (!pageId) return
  const summary = await previewCowartMigration(app, pageId)
  if (!window.confirm(`Create ${summary.targetCanvasId} without changing the old page?`)) return
  onResult(await confirmCowartMigration(app, pageId, summary.targetCanvasId))
}

function choosePage(pages: Array<{ pageId: string; sourcePath: string; updatedAt: string }>): string | undefined {
  const labels = pages.map((page, index) => `${index + 1}. ${page.pageId} (${page.updatedAt})`).join("\n")
  const choice = window.prompt(`Choose a Cowart page:\n${labels}`, "1")?.trim()
  const index = Number(choice) - 1
  return Number.isInteger(index) && index >= 0 && index < pages.length ? pages[index]?.pageId : undefined
}
