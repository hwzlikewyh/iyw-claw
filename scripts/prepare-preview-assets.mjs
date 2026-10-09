import { cpSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"

const root = process.cwd()
const pdfTarget = join(root, "public/preview-assets/pdf")
mkdirSync(pdfTarget, { recursive: true })
for (const name of ["cmaps", "standard_fonts", "wasm"]) {
  cpSync(join(root, "node_modules/pdfjs-dist", name), join(pdfTarget, name), {
    recursive: true,
  })
}
// worker 没有页面的全局对象，必须独立加载相同的 PDF 兼容接口。
writeFileSync(
  join(pdfTarget, "pdf.worker.min.mjs"),
  [
    readFileSync(join(root, "src/lib/pdf-compatibility.mjs"), "utf8"),
    readFileSync(
      join(root, "node_modules/pdfjs-dist/legacy/build/pdf.worker.min.mjs"),
      "utf8"
    ),
  ].join("\n")
)
for (const name of ["draco/gltf", "basis"]) {
  const target = join(root, "public/preview-assets/three", name)
  mkdirSync(target, { recursive: true })
  cpSync(join(root, "node_modules/three/examples/jsm/libs", name), target, {
    recursive: true,
  })
}
