import { readFile, writeFile } from "node:fs/promises"
import path from "node:path"

export async function adaptPanel(fork) {
  const directory = path.join(fork, "dist/ui")
  const panelFile = path.join(directory, "panel.html")
  await writeFile(
    panelFile,
    (await readFile(panelFile, "utf8")).replaceAll(
      '"zcode/surface"',
      '"iyw/surface"'
    )
  )
  const indexFile = path.join(directory, "assets.json")
  const index = JSON.parse(await readFile(indexFile, "utf8"))
  let bytes = 0
  for (const file of index.scripts) {
    const scriptPath = path.join(directory, file)
    let script = await readFile(scriptPath, "utf8")
    const inline = /([A-Za-z_$][\w$]*)\?\.displayMode==="inline"/g
    if ([...script.matchAll(inline)].length !== 1)
      throw Error("Upstream panel layout changed")
    script = script.replace(
      inline,
      '( $& && window.__officeHost?.capabilities?.experimental?.["iyw/surface"]?.kind!=="panel")'
    )
    script = script.replaceAll('"zcode/surface"', '"iyw/surface"')
    script = adaptRendering(script)
    await writeFile(scriptPath, script)
    bytes += Buffer.byteLength(script)
  }
  index.scriptBytes = bytes
  await writeFile(indexFile, JSON.stringify(index))
}

function adaptRendering(script) {
  if (
    script.includes("__officeXlsxWorkerUrl") ||
    script.includes('"pdf-worker.js"')
  ) {
    const workerUrl =
      'URL.createObjectURL(new Blob([t],{type:"text/javascript"}))'
    if (script.split(workerUrl).length !== 2)
      throw Error("Upstream module worker asset changed")
    script = script.replace(
      workerUrl,
      '("data:text/javascript;charset=utf-8,"+encodeURIComponent(t))'
    )
  }
  if (script.includes("viewer-scroll docx-scroll")) {
    const dimensions =
      /if\(!([A-Za-z_$][\w$]*)\|\|!([A-Za-z_$][\w$]*)\.width\)return 1;let/g
    const zoom =
      /if\(([A-Za-z_$][\w$]*)\.current=([A-Za-z_$][\w$]*),([A-Za-z_$][\w$]*)&&([A-Za-z_$][\w$]*)\.current\.onZoom\?\./g
    const sizes = [...script.matchAll(dimensions)],
      updates = [...script.matchAll(zoom)]
    if (
      sizes.length !== 1 ||
      updates.length !== 1 ||
      sizes[0][1] !== updates[0][3]
    )
      throw Error("Upstream DOCX initial fit changed")
    script = script.replace(
      zoom,
      `if($1.current=$3&&${sizes[0][2]}.width?$2:0,$3&&$4.current.onZoom?.`
    )
  }
  return script
}
