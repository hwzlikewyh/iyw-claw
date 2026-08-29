export function renderMarkdown(source: string): string {
  const escaped = escapeHtml(source)
  return escaped
    .replace(/^### (.+)$/gm, "<h3>$1</h3>")
    .replace(/^## (.+)$/gm, "<h2>$1</h2>")
    .replace(/^# (.+)$/gm, "<h1>$1</h1>")
    .replace(/^[-*] (.+)$/gm, "<li>$1</li>")
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/\*(.+?)\*/g, "<em>$1</em>")
    .replace(/\n{2,}/g, "</p><p>")
    .replace(/^(?!<h|<li|<p|$)(.+)$/gm, "<p>$1</p>")
}

export const markdownPlugin = { type: "iyw:markdown", render: renderMarkdown }

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" })[character] ?? character)
}
