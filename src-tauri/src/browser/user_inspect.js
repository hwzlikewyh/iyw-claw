;(() => {
  const [x, y] = __POINT__
  const clip = (value, limit) => {
    const text = value.slice(0, limit)
    const last = text.charCodeAt(text.length - 1)
    return last >= 0xd800 && last <= 0xdbff ? text.slice(0, -1) : text
  }
  let element = document.elementFromPoint(x, y)
  if (!element) return null
  for (let depth = 0; depth < 8 && element.shadowRoot; depth += 1) {
    const child = element.shadowRoot.elementFromPoint(x, y)
    if (!child || child === element) break
    element = child
  }
  const text = clip(
    (element.innerText || element.textContent || "")
      .trim()
      .replace(/\s+/g, " "),
    400
  )
  const tag = element.tagName.toLowerCase()
  const selector = element.id
    ? "#" + CSS.escape(element.id)
    : tag +
      Array.from(element.classList)
        .slice(0, 3)
        .map((value) => "." + CSS.escape(value))
        .join("")
  const rect = element.getBoundingClientRect()
  const clone = element.cloneNode(true)
  for (const input of [clone, ...clone.querySelectorAll("input, textarea")]) {
    if (input.matches('input[type="password"], input[type="hidden"]')) {
      input.removeAttribute("value")
    }
  }
  return {
    tag,
    role: element.getAttribute("role"),
    name:
      clip(
        element.getAttribute("aria-label") ||
          element.getAttribute("title") ||
          text ||
          "",
        400
      ) || null,
    text: text || null,
    selector: clip(selector, 512),
    html: clip(clone.outerHTML, 8192),
    bounds: [rect.x, rect.y, rect.width, rect.height],
    viewport: [window.innerWidth, window.innerHeight],
  }
})()
