import type { MigrationSummary } from "./cowart-migration-dialog.js"

export function renderMigrationResult(summary: MigrationSummary & { reportPath?: string }): HTMLElement {
  const result = document.createElement("section")
  result.innerHTML = `<h2>Migration complete</h2><p>${summary.mapped} mapped, ${summary.skipped} skipped</p>${summary.reportPath ? `<code>${summary.reportPath}</code>` : ""}`
  if (summary.warnings.length) { const list = document.createElement("ul"); for (const warning of summary.warnings) { const item = document.createElement("li"); item.textContent = warning; list.append(item) }; result.append(list) }
  return result
}
