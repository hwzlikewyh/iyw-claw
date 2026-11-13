import type { CanvasNodeData, CanvasNodeImage } from "@/types/canvas"
import { uploadImage } from "./image-storage"
import { imageMetadata } from "@/lib/canvas/canvas-node-factory"
import { fitNodeSize } from "@/lib/canvas/canvas-node-size"

type Update = (change: (nodes: CanvasNodeData[]) => CanvasNodeData[]) => void
const pending = new Set<string>()

export async function recoverCanvasImage(node: CanvasNodeData, input: { imageId?: string; update: Update }): Promise<boolean> {
  const images = (node.metadata?.images || []).filter((image) => image.recoverySourceUrl && (!input.imageId || image.id === input.imageId))
  const source = !images.length && !input.imageId ? node.metadata?.recoverySourceUrl : undefined
  if (!images.length && !source) return false
  const key = `${node.id}:${input.imageId || "node"}`
  if (pending.has(key)) return true
  pending.add(key)
  try {
    if (source) await recoverNode(node, source, input.update)
    for (const image of images) await recoverSlot(node, image, input.update)
  } finally { pending.delete(key) }
  return true
}

async function recoverNode(node: CanvasNodeData, source: string, update: Update): Promise<void> {
  update((nodes) => nodes.map((item) => item.id === node.id ? { ...item, metadata: { ...item.metadata, status: "loading" } } : item))
  try {
    const image = await uploadImage(source)
    update((nodes) => nodes.map((item) => item.id === node.id ? { ...item, metadata: { ...item.metadata, ...imageMetadata(image), recoverySourceUrl: undefined, errorDetails: undefined } } : item))
  } catch (error) {
    update((nodes) => nodes.map((item) => item.id === node.id ? { ...item, metadata: { ...item.metadata, status: "error", errorDetails: (error as Error).message } } : item))
  }
}

async function recoverSlot(node: CanvasNodeData, image: CanvasNodeImage, update: Update): Promise<void> {
  const setSlot = (patch: Partial<CanvasNodeImage>) => update((nodes) => nodes.map((item) => item.id === node.id ? { ...item, metadata: { ...item.metadata, images: item.metadata?.images?.map((slot) => slot.id === image.id ? { ...slot, ...patch } : slot) } } : item))
  setSlot({ status: "loading", errorDetails: undefined })
  try {
    const stored = await uploadImage(image.recoverySourceUrl!)
    const slot: CanvasNodeImage = { id: image.id, status: "success", content: stored.url, storageKey: stored.storageKey, naturalWidth: stored.width, naturalHeight: stored.height, bytes: stored.bytes, mimeType: stored.mimeType }
    update((nodes) => nodes.map((item) => {
      if (item.id !== node.id) return item
      const images = item.metadata?.images?.map((entry) => entry.id === image.id ? slot : entry)
      const primary = !item.metadata?.primaryImageId || item.metadata.primaryImageId === image.id
      return { ...item, ...(primary ? fitNodeSize(stored.width, stored.height, node.width, node.height) : {}), metadata: { ...item.metadata, ...(primary ? imageMetadata(stored) : {}), images, primaryImageId: primary ? image.id : item.metadata?.primaryImageId, status: "success", errorDetails: undefined } }
    }))
  } catch (error) { setSlot({ status: "error", errorDetails: (error as Error).message }) }
}
