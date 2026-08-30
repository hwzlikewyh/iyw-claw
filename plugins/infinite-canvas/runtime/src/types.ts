export type CanvasNodeData = {
  id: string
  type: string
  x: number
  y: number
  width: number
  height: number
  rotation?: number
  metadata?: Record<string, unknown>
  [key: string]: unknown
}

export const MAX_SCENE_NODES = 10_000
export const MAX_SCENE_CONNECTIONS = 20_000
export const MAX_NODE_METADATA_BYTES = 256 * 1024

export type CanvasConnection = {
  id: string
  fromNodeId: string
  toNodeId: string
}

export type CanvasScene = {
  schemaVersion: 1
  canvasId: string
  revision: number
  nodes: CanvasNodeData[]
  connections: CanvasConnection[]
  backgroundMode: "dots" | "lines" | "blank"
  showImageInfo: boolean
  viewport: { x: number; y: number; k: number }
  updatedAt: string
}

export type CanvasSelection = {
  revision: number
  selectedNodeIds: string[]
  updatedAt: string
}

export type CanvasOperation =
  | { type: "add_node"; node: CanvasNodeData }
  | { type: "update_node"; nodeId: string; patch: Record<string, unknown> }
  | { type: "remove_node"; nodeId: string }
  | { type: "add_connection"; connection: CanvasConnection }
  | { type: "remove_connection"; connectionId: string }
  | { type: "set_viewport"; viewport: CanvasScene["viewport"] }

export type AssetRef = {
  sha256: string
  mimeType: string
  bytes: number
  path: string
}

export function defaultScene(canvasId: string): CanvasScene {
  return {
    schemaVersion: 1,
    canvasId,
    revision: 0,
    nodes: [],
    connections: [],
    backgroundMode: "dots",
    showImageInfo: true,
    viewport: { x: 0, y: 0, k: 1 },
    updatedAt: new Date(0).toISOString(),
  }
}
