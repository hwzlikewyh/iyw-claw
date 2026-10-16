import type { BrowserElementSummary } from "@/lib/browser-types"
import type { ReferenceAttrs } from "./types"

const ELEMENT_URI_PREFIX = "iyw-claw://element/"
type ElementType =
  | "button"
  | "heading"
  | "region"
  | "link"
  | "input"
  | "image"
  | "text"
  | "element"

export function readElementReference(uri: string | null) {
  if (!uri?.toLowerCase().startsWith(ELEMENT_URI_PREFIX)) return null
  try {
    const parsed = new URL(uri)
    const tag = decodeURIComponent(parsed.pathname.slice(1)).toLowerCase()
    const selector = parsed.searchParams.get("selector") ?? ""
    if (!/^[a-z][a-z0-9-]*$/.test(tag) || !selector.trim()) return null
    return {
      tag,
      selector,
      role: parsed.searchParams.get("role") || undefined,
      text: parsed.searchParams.get("text") || undefined,
      html: parsed.searchParams.get("html") || undefined,
      url: parsed.searchParams.get("url") || undefined,
    }
  } catch {
    return null
  }
}

export function parseElementReferenceUri(
  uri: string,
  label: string
): ReferenceAttrs | null {
  const element = readElementReference(uri)
  if (!element) return null
  return {
    refType: "element",
    id: uri,
    label: label || element.text || element.selector,
    uri,
    meta: null,
  }
}

export function buildElementReference(
  element: BrowserElementSummary,
  url: string
): ReferenceAttrs {
  const query = new URLSearchParams({ selector: element.selector, url })
  if (element.role) query.set("role", element.role)
  if (element.text) query.set("text", element.text)
  if (element.html) query.set("html", element.html)
  const tag = element.tag.trim().toLowerCase()
  const uri = `${ELEMENT_URI_PREFIX}${encodeURIComponent(tag)}?${query}`
  return {
    refType: "element",
    id: uri,
    label: element.name || element.text || element.selector,
    uri,
    meta: null,
  }
}

export function elementReferenceType(tag = "", role = ""): ElementType {
  const roles: Record<string, ElementType> = {
    button: "button",
    heading: "heading",
    link: "link",
    textbox: "input",
    img: "image",
    region: "region",
    navigation: "region",
  }
  const tags: Record<string, ElementType> = {
    button: "button",
    a: "link",
    input: "input",
    textarea: "input",
    select: "input",
    img: "image",
    svg: "image",
    div: "region",
    section: "region",
    article: "region",
    main: "region",
    nav: "region",
    p: "text",
    span: "text",
  }
  const roleType = Object.prototype.hasOwnProperty.call(roles, role)
    ? roles[role]
    : undefined
  const tagType = Object.prototype.hasOwnProperty.call(tags, tag)
    ? tags[tag]
    : undefined
  return roleType || (/^h[1-6]$/.test(tag) ? "heading" : tagType) || "element"
}
