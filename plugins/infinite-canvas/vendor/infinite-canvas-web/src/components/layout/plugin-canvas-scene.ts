import { CanvasNodeType, type CanvasNodeData } from "@/types/canvas"
import { useCanvasStore, type CanvasProject } from "@/stores/canvas/use-canvas-store"
import { setImageBlob } from "@/services/image-storage"
import { setMediaBlob } from "@/services/file-storage"
import { getStoredAssetReferences } from "../../../../../widget/src/upstream/plugin-storage"
import type { AssetClient, AssetReference } from "../../../../../widget/src/asset-client"
import type { App } from "@modelcontextprotocol/ext-apps"
import { useAgentStore } from "@/stores/use-agent-store"
import type { CanvasAgentOp } from "@/lib/canvas/canvas-agent-ops"

type BridgeHost = Pick<App, "callServerTool">
export type CanvasScene = {
  canvasId: string
  revision: number
  nodes: Array<Record<string, unknown>>
  connections: CanvasProject["connections"]
  backgroundMode: CanvasProject["backgroundMode"]
  showImageInfo: boolean
  viewport: CanvasProject["viewport"]
  updatedAt: string
}

export function syncConflict(): Error { return new Error("Canvas changed in another Agent. Reopen the canvas before continuing.") }

export async function projectToScene(project: CanvasProject, scene: CanvasScene, app: BridgeHost): Promise<CanvasScene> {
  const ids = new Map(project.nodes.map((node, index) => [node.id, sceneNodeId(node.id, index)]))
  const [images, media] = await Promise.all([
    getStoredAssetReferences(app, "image_files"),
    getStoredAssetReferences(app, "media_files"),
  ])
  const nodes = project.nodes.map((node) => projectNode(node, { ids, images, media }))
  return {
    schemaVersion: 1,
    canvasId: scene.canvasId,
    revision: scene.revision,
    nodes,
    connections: project.connections.map((connection) => ({
      ...connection,
      id: sceneNodeId(connection.id, project.connections.indexOf(connection)),
      fromNodeId: ids.get(connection.fromNodeId) || sceneNodeId(connection.fromNodeId, 0),
      toNodeId: ids.get(connection.toNodeId) || sceneNodeId(connection.toNodeId, 0),
    })),
    backgroundMode: project.backgroundMode,
    showImageInfo: project.showImageInfo,
    viewport: project.viewport,
    updatedAt: project.updatedAt,
  } as CanvasScene
}


function projectNode(node: CanvasNodeData, context: { ids: Map<string, string>; images: Map<string, AssetReference>; media: Map<string, AssetReference> }): Record<string, unknown> {
  const { ids, images, media } = context
  const metadata = sanitizeMetadata({ ...(node.metadata || {}), title: node.title })
  const storageKey = typeof metadata.storageKey === "string" ? metadata.storageKey : ""
  const mediaType = node.type === CanvasNodeType.Image ? "image" : node.type === CanvasNodeType.Video ? "video" : node.type === CanvasNodeType.Audio ? "audio" : undefined
  const asset = storageKey ? (mediaType === "image" ? images : media).get(storageKey) : undefined
  if (typeof metadata.content === "string" && metadata.content.startsWith("blob:")) delete metadata.content
  if (asset) metadata.asset = asset
  if (mediaType) metadata.mediaType = mediaType
  if (ids.get(node.id) !== node.id) metadata.originalNodeId = node.id
  const position = node.position || { x: 0, y: 0 }
  return {
    id: ids.get(node.id),
    type: mediaType ? "media" : node.type,
    x: position.x,
    y: position.y,
    width: node.width,
    height: node.height,
    ...(metadata.fontSize ? { fontSize: metadata.fontSize } : {}),
    metadata,
  }
}

export async function sceneToProject(scene: CanvasScene, assets: AssetClient, identity: { id: string; title: string }): Promise<CanvasProject> {
  const nodes = await Promise.all(scene.nodes.map((node) => sceneNodeToCanvasNode(node, assets)))
  const now = new Date().toISOString()
  return {
    ...identity,
    createdAt: now,
    updatedAt: scene.updatedAt,
    nodes,
    connections: scene.connections,
    chatSessions: [],
    activeChatId: null,
    backgroundMode: scene.backgroundMode,
    showImageInfo: scene.showImageInfo,
    viewport: scene.viewport,
  }
}

async function sceneNodeToCanvasNode(node: Record<string, unknown>, assets: AssetClient): Promise<CanvasNodeData> {
  const metadata = { ...((node.metadata as Record<string, unknown> | undefined) || {}) }
  const asset = metadata.asset as AssetReference | undefined
  let type = String(node.type || "text")
  if (asset) {
    const url = await assets.getUrl(asset)
    const blob = await fetch(url).then((response) => response.blob())
    const media = asset.mimeType.startsWith("image/")
      ? { type: CanvasNodeType.Image, storageKey: `image:${asset.sha256}`, url: await setImageBlob(`image:${asset.sha256}`, blob) }
      : asset.mimeType.startsWith("video/")
        ? { type: CanvasNodeType.Video, storageKey: `video:${asset.sha256}`, url: await setMediaBlob(`video:${asset.sha256}`, blob) }
        : { type: CanvasNodeType.Audio, storageKey: `audio:${asset.sha256}`, url: await setMediaBlob(`audio:${asset.sha256}`, blob) }
    type = media.type
    metadata.storageKey = media.storageKey
    metadata.content = media.url
  } else if (type === "media") {
    type = metadata.mediaType === "video" ? CanvasNodeType.Video : CanvasNodeType.Image
  }
  if (typeof metadata.text === "string" && typeof metadata.content !== "string") metadata.content = metadata.text
  return {
    id: String(metadata.originalNodeId || node.id),
    type,
    title: String(metadata.title || node.title || type),
    position: { x: Number(node.x) || 0, y: Number(node.y) || 0 },
    width: Number(node.width) || 240,
    height: Number(node.height) || 120,
    metadata: metadata as CanvasNodeData["metadata"],
  }
}

function sceneNodeId(id: string, index: number): string {
  return /^[A-Za-z0-9_-]{1,64}$/.test(id) ? id : `legacy-${index}`
}

function sanitizeMetadata(value: Record<string, unknown>): Record<string, unknown> {
  const metadata = structuredClone(value)
  for (const key of ["content", "dataUrl", "url"]) {
    if (typeof metadata[key] === "string" && /^blob:|^data:/i.test(metadata[key] as string)) delete metadata[key]
  }
  if (Array.isArray(metadata.images)) {
    metadata.images = metadata.images.map((item) => {
      if (!item || typeof item !== "object") return item
      const image = { ...item as Record<string, unknown> }
      for (const key of ["content", "dataUrl", "url"]) {
        if (typeof image[key] === "string" && /^blob:|^data:/i.test(image[key] as string)) delete image[key]
      }
      return image
    })
  }
  return metadata
}

export async function applySceneToProject(scene: CanvasScene, project: CanvasProject, assets: AssetClient): Promise<void> {
  const next = await sceneToProject(scene, assets, { id: project.id, title: project.title })
  const current = useCanvasStore.getState().projects.find((item) => item.id === project.id)
  if (!current || projectSignature(current) !== projectSignature(project)) throw syncConflict()
  applyToOpenCanvas(next, project)
  useCanvasStore.getState().updateProject(project.id, {
    nodes: next.nodes,
    connections: next.connections,
    backgroundMode: next.backgroundMode,
    showImageInfo: next.showImageInfo,
    viewport: next.viewport,
  })
}

function applyToOpenCanvas(next: CanvasProject, previous: CanvasProject): void {
  const context = useAgentStore.getState().canvasContext
  if (!context || context.snapshot.projectId !== next.id) return
  const snapshot = context.snapshot
  if (projectSignature({ ...previous, ...snapshot, id: previous.id }) !== projectSignature(previous)) throw syncConflict()
  const remaining = new Set(next.nodes.map((node) => node.id))
  const operations: CanvasAgentOp[] = [
    { type: "delete_node", ids: snapshot.nodes.map((node) => node.id) },
    ...next.nodes.map((node): CanvasAgentOp => ({ type: "add_node", id: node.id, nodeType: node.type, title: node.title, position: node.position, width: node.width, height: node.height, metadata: node.metadata })),
    ...next.connections.map((connection): CanvasAgentOp => ({ type: "connect_nodes", ...connection })),
    { type: "set_viewport", viewport: next.viewport },
    { type: "select_nodes", ids: snapshot.selectedNodeIds.filter((id) => remaining.has(id)) },
  ]
  context.applyOps(operations)
}

export function projectSignature(project: CanvasProject): string {
  const nodes = project.nodes.map((node) => ({ ...node, metadata: sanitizeMetadata(node.metadata || {}) }))
  return JSON.stringify([nodes, project.connections, project.backgroundMode, project.showImageInfo, project.viewport])
}
