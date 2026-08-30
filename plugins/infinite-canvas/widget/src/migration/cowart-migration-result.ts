import type { MigrationSummary } from "./cowart-migration-dialog.js"

export function renderMigrationResult(summary: MigrationSummary & { reportPath?: string }): HTMLElement {
  const result = document.createElement("section")
  const heading = document.createElement("h2")
  heading.textContent = "Migration complete"
  const counts = document.createElement("p")
  counts.textContent = `${summary.mapped} mapped, ${summary.skipped} skipped`
  result.append(heading, counts)
  if (summary.reportPath) { const path = document.createElement("code"); path.textContent = summary.reportPath; result.append(path) }
  if (summary.warnings.length) { const list = document.createElement("ul"); for (const warning of summary.warnings) { const item = document.createElement("li"); item.textContent = warning; list.append(item) }; result.append(list) }
  return result
}
