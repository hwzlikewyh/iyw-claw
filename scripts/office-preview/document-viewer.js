// 在当前沙箱内隔离文档样式，避免读取另一个不透明来源的 iframe。
LazyHcdViewer.prototype.createDocument = function (styles) {
  const root = frame.shadowRoot || frame.attachShadow({ mode: "open" })
  const html = document.createElement("html")
  const head = document.createElement("head")
  const body = document.createElement("body")
  const style = document.createElement("style")
  style.textContent = `
    html{display:block;background:#eef1f5;min-height:100%}
    body{box-sizing:border-box;max-width:max-content;min-width:min(100%,960px);margin:24px auto;padding:24px;background:#fff;color:#111;box-shadow:0 3px 18px #0002}
    .hcd-lazy-placeholder{display:block;box-sizing:border-box;min-height:480px;margin-bottom:12px;background:#f4f6f8;border:1px solid #d7dce2}
    .hcd-lazy-error{padding:18px;color:#9f1239;background:#fff1f2;border:1px solid #fda4af}
    #hcd-lazy-tail{height:2px}
  ` + styles.replace(/asset:\/\/sha256\/([0-9a-f]{64})/g, (source, hash) => this.assets.get(hash) ?? source)
  body.dataset.hcdProfile = this.manifest.profile
  body.dataset.hcdSourceFormat = this.manifest.source.format
  body.dataset.hcdRevision = String(this.revision)
  body.dataset.hcdTextHitboxes = "off"
  body.dataset.hcdImageHitboxes = "off"
  if (this.manifest.source.format === "docx") style.textContent += wordPageCss
  if (this.manifest.source.format === "pptx") style.textContent += slidePageCss
  head.append(style)
  html.append(head, body)
  root.replaceChildren(html)
  this.doc = { body, createElement: (tag) => document.createElement(tag) }
  this.wordHead = head
  if (this.manifest.source.format === "pptx") {
    this.slideObserver = new ResizeObserver(() => sizeSlides(body, frame.clientWidth))
    this.slideObserver.observe(frame)
  }
  this.tail = document.createElement("div")
  this.tail.id = "hcd-lazy-tail"
  body.append(this.tail)
  this.observer = new IntersectionObserver(
    (entries) => this.onIntersection(entries),
    { root: frame, rootMargin: "120% 0px 120% 0px" }
  )
}

// 上游示例的分片统计不属于产品提示；异步渲染错误交给宿主统一显示。
LazyHcdViewer.prototype.updateStatus = function () {}
LazyHcdViewer.prototype.fail = function (error) {
  if (!this.abort.signal.aborted) window.officeReportError(error)
}
const showChunkError = LazyHcdViewer.prototype.showChunkError
LazyHcdViewer.prototype.showChunkError = function (slot, error) {
  if (this.abort.signal.aborted) return
  showChunkError.call(this, slot, error)
  this.fail(error)
}

LazyHcdViewer.prototype.preparePresentation = function (node, descriptor) {
  const result = this.manifest.source.format === "pptx"
    ? prepareSlide(node, descriptor.region === "note") : node
  requestAnimationFrame(() => {
    if (!this.abort.signal.aborted) sizeSlides(this.doc.body, frame.clientWidth)
  })
  return result
}
const closeDocument = LazyHcdViewer.prototype.close
LazyHcdViewer.prototype.close = function () {
  this.slideObserver?.disconnect()
  closeDocument.call(this)
}

let officeViewer
const RESIDENT_CHUNKS = 12
// 分页需要所有段落同时参与测量。大型文件继续通过可见分片控制内存。
const MAX_LAYOUT_CHUNKS = 64
const MAX_LAYOUT_NODES = 20_000
const MAX_LAYOUT_BYTES = 8 * 1024 * 1024

async function layoutWord(viewer) {
  const { manifest, slots, abort } = viewer
  if (manifest.source.format !== "docx" || manifest.chunkCount > MAX_LAYOUT_CHUNKS) return
  const all = [...slots.values()]
  if (!all.length || all.length !== manifest.chunkCount) return
  const sum = (key) => all.reduce((total, slot) => total + slot.descriptor[key], 0)
  if (sum("nodeCount") > MAX_LAYOUT_NODES || sum("byteLength") > MAX_LAYOUT_BYTES) return
  viewer.residentLimit = all.length
  viewer.observer.disconnect()
  viewer.tailObserver?.disconnect()
  for (const slot of all) {
    await viewer.loadChunk(slot)
    abort.signal.throwIfAborted()
  }
  viewer.observer.disconnect()
  const body = viewer.doc.body
  await Promise.all([...body.querySelectorAll("img")].map((image) => image.decode().catch(() => {})))
  await document.fonts.ready
  abort.signal.throwIfAborted()
  paginateWord(body, viewer.wordHead)
}

window.officeLoad = async function (base) {
  const top = frame.scrollTop
  officeViewer?.close()
  officeViewer = new LazyHcdViewer(new URL(base), undefined, RESIDENT_CHUNKS)
  const viewer = officeViewer
  await viewer.open()
  await layoutWord(viewer)
  if (viewer.abort.signal.aborted) return
  frame.scrollTo(0, top)
}
window.officeDispose = () => officeViewer?.close()
