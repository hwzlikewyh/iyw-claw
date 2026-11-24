const slidePageCss = `
body[data-hcd-source-format="pptx"]{display:block!important;box-sizing:border-box;width:100%!important;max-width:none!important;min-width:0!important;margin:0!important;padding:16px!important;background:#eef1f5;box-shadow:none}
.office-slide-page{position:relative;margin:0 auto 16px;overflow:hidden;box-shadow:0 2px 10px #0002}
.office-slide-page>.hcd-slide{position:absolute!important;left:0;top:0;margin:0!important;transform-origin:top left}
.hcd-slide-text{flex-shrink:0}
.office-slide-notes{box-sizing:border-box;margin:0 auto 16px;padding:10px 14px;background:white;white-space:pre-wrap;overflow-wrap:anywhere;font:14px/1.5 "Noto Sans SC",sans-serif}
.office-slide-notes summary{cursor:pointer}
.office-slide-notes .hcd-slide,.office-slide-notes .hcd-slide-shape{position:static!important;width:auto!important;height:auto!important;min-height:0!important;background:white!important;color:#222}
@media print{body[data-hcd-source-format="pptx"]{padding:0!important}.office-slide-page{margin:0;box-shadow:none;break-after:page}.office-slide-page:last-child{break-after:auto}.office-slide-page>.hcd-slide{transform:none!important}.office-slide-notes{display:none}}
`

function prepareSlide(node, notes = false) {
  if (!node.matches(".hcd-slide")) return node
  if (notes || /\/notesSlides\//.test(node.dataset.hcdSourcePart || "")) {
    const detail = node.ownerDocument.createElement("details")
    detail.className = "office-slide-notes"
    const summary = node.ownerDocument.createElement("summary")
    const number = node.dataset.hcdSourcePart?.match(/notesSlide(\d+)\.xml$/)?.[1] || ""
    summary.textContent = navigator.language.startsWith("zh") ? "演讲者备注 " + number : "Speaker notes " + number
    detail.append(summary, node)
    return detail
  }
  const page = node.ownerDocument.createElement("section")
  page.className = "office-slide-page"
  page.append(node)
  // 字体需要在节点接入文档后再按实际字体解析。
  requestAnimationFrame(() => applyWindowsNotoFace(node))
  return page
}

function sizeSlides(body, width) {
  const padding = parseFloat(getComputedStyle(body).paddingLeft) + parseFloat(getComputedStyle(body).paddingRight)
  const available = Math.max(1, width - padding)
  for (const page of body.querySelectorAll(".office-slide-page")) {
    const slide = page.firstElementChild
    const w = parseFloat(slide.style.width), h = parseFloat(slide.style.height)
    if (!(w > 0 && h > 0)) continue
    const scale = Math.min(1, available / w)
    page.style.width = w * scale + "px"
    page.style.height = h * scale + "px"
    slide.style.transform = "scale(" + scale + ")"
  }
}

function standaloneSlides(body, head) {
  if (body.dataset.hcdSourceFormat !== "pptx") return
  const style = body.ownerDocument.createElement("style")
  style.textContent = slidePageCss
  head.append(style)
  for (const slide of [...body.querySelectorAll(":scope>.hcd-slide")]) {
    const marker = body.ownerDocument.createComment("slide")
    slide.before(marker)
    marker.replaceWith(prepareSlide(slide))
  }
  const resize = () => sizeSlides(body, body.clientWidth)
  new ResizeObserver(resize).observe(body)
  resize()
  const first = body.querySelector(".office-slide-page>.hcd-slide")
  if (first) {
    const w = first.style.width, h = first.style.height
    style.textContent += "@page{size:" + w + " " + h + ";margin:0}@media print{.office-slide-page{width:" + w + "!important;height:" + h + "!important}}"
  }
}
