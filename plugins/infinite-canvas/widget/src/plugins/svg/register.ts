export function sanitizeSvg(source: string): string {
  const parsed = new DOMParser().parseFromString(source, "image/svg+xml")
  if (parsed.querySelector("parsererror")) return ""
  for (const element of parsed.querySelectorAll("script,foreignObject,iframe,object,embed,form")) element.remove()
  for (const element of parsed.querySelectorAll("[href],[xlink\\:href]")) {
    for (const attribute of ["href", "xlink:href"]) {
      const value = element.getAttribute(attribute)
      if (value && !value.startsWith("data:")) element.removeAttribute(attribute)
    }
  }
  return new XMLSerializer().serializeToString(parsed.documentElement)
}

export const svgPlugin = { type: "iyw:svg", sanitize: sanitizeSvg }
