import { sanitizeHtml } from "./html-actions.js"

export const htmlPlugin = {
  type: "iyw:html",
  sanitize: sanitizeHtml,
  sandbox: { scripts: false, forms: false, navigation: false },
}
