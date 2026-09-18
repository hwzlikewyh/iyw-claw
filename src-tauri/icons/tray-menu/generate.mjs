import { createRequire } from "node:module"
import { fileURLToPath } from "node:url"
import { createElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"
import {
  Monitor,
  MessageSquarePlus,
  FolderOpen,
  Settings,
  RefreshCw,
  Power,
} from "lucide-react"

const require = createRequire(import.meta.url)
const sharp = require(
  require.resolve("sharp", { paths: [require.resolve("next")] })
)
const icons = {
  workspace: Monitor,
  conversation: MessageSquarePlus,
  folder: FolderOpen,
  settings: Settings,
  update: RefreshCw,
  quit: Power,
}
const ICON_SIZE = 16

for (const [name, icon] of Object.entries(icons)) {
  const svg = renderToStaticMarkup(
    createElement(icon, {
      size: ICON_SIZE,
      color: "#6b7280",
      strokeWidth: 1.75,
    })
  )
  await sharp(Buffer.from(svg))
    .png()
    .toFile(fileURLToPath(new URL(`${name}.png`, import.meta.url)))
}
