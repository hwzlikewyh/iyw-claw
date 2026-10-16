import { isDesktop } from "@/lib/platform"

const OBJECT_URL_LIFETIME_MS = 1000

type UiFile = {
  name: string
  content: string
  type: string
}

export async function downloadUiFile(input: UiFile): Promise<boolean> {
  if (!isDesktop()) return downloadBrowserFile(input)
  const { save } = await import("@tauri-apps/plugin-dialog")
  const extension = input.name.split(".").pop() || "txt"
  const path = await save({
    defaultPath: input.name,
    filters: [{ name: extension.toUpperCase(), extensions: [extension] }],
  })
  if (!path) return false
  const { invoke } = await import("@tauri-apps/api/core")
  await invoke("save_text_file", { path, contents: input.content })
  console.info("[intelligent-ui] file exported", {
    name: input.name,
    mode: "desktop",
  })
  return true
}

function downloadBrowserFile(input: UiFile): boolean {
  const url = URL.createObjectURL(
    new Blob([input.content], { type: input.type })
  )
  const link = document.createElement("a")
  link.href = url
  link.download = input.name
  try {
    document.body.append(link)
    link.click()
    return true
  } finally {
    link.remove()
    setTimeout(() => URL.revokeObjectURL(url), OBJECT_URL_LIFETIME_MS)
  }
}
export function exportUiSource(
  source: string,
  state: Readonly<Record<string, unknown>>
) {
  return downloadUiFile({
    name: "intelligent-ui.json",
    content: JSON.stringify({ version: 2, source, state }, null, 2),
    type: "application/json;charset=utf-8",
  })
}
