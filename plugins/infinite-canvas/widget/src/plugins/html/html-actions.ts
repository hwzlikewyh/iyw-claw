import type { App } from "@modelcontextprotocol/ext-apps"
import { newCreativeRequest, sendCreativeRequest } from "../../creative-request.js"

export function sanitizeHtml(source: string): string {
  const documentValue = new DOMParser().parseFromString(source, "text/html")
  for (const element of documentValue.querySelectorAll("script,iframe,form,object,embed,base,link")) element.remove()
  for (const element of documentValue.querySelectorAll("[src],[href],[action]")) {
    for (const attribute of ["src", "href", "action"]) {
      const value = element.getAttribute(attribute)
      if (value && !value.startsWith("#") && !value.startsWith("data:")) element.removeAttribute(attribute)
    }
  }
  for (const element of documentValue.querySelectorAll("*")) {
    for (const attribute of element.getAttributeNames()) if (attribute.toLowerCase().startsWith("on")) element.removeAttribute(attribute)
    const style = element.getAttribute("style")
    if (style && /url\s*\(|expression\s*\(/i.test(style)) element.removeAttribute("style")
  }
  for (const element of documentValue.querySelectorAll("style")) {
    const source = element.textContent ?? ""
    if (/@import|url\s*\(|expression\s*\(/i.test(source)) element.remove()
  }
  return `<!doctype html>${documentValue.documentElement.outerHTML}`
}

export async function requestHtml(app: App, canvasId: string, prompt: string, targetNodeId?: string, revision = 0): Promise<void> {
  const request = newCreativeRequest(targetNodeId ? "html.edit" : "html.generate", canvasId, prompt, [], targetNodeId, revision)
  await sendCreativeRequest(app, request)
}
