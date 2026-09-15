import { cpSync, mkdirSync } from "node:fs"
import { join } from "node:path"

const root = process.cwd()
const pdfTarget = join(root, "public/preview-assets/pdf")
mkdirSync(pdfTarget, { recursive: true })
for (const name of ["cmaps", "standard_fonts", "wasm"]) {
  cpSync(join(root, "node_modules/pdfjs-dist", name), join(pdfTarget, name), {
    recursive: true,
  })
}
cpSync(
  join(root, "node_modules/pdfjs-dist/build/pdf.worker.min.mjs"),
  join(pdfTarget, "pdf.worker.min.mjs")
)
for (const name of ["draco/gltf", "basis"]) {
  const target = join(root, "public/preview-assets/three", name)
  mkdirSync(target, { recursive: true })
  cpSync(join(root, "node_modules/three/examples/jsm/libs", name), target, {
    recursive: true,
  })
}
