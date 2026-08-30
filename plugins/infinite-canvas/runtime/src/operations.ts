import { CanvasRuntimeError, invalid } from "./errors.js"
import { MAX_NODE_METADATA_BYTES, MAX_SCENE_CONNECTIONS, MAX_SCENE_NODES, type CanvasOperation, type CanvasScene, type CanvasNodeData, type CanvasConnection } from "./types.js"

const FORBIDDEN_KEYS = new Set(["__proto__", "prototype", "constructor"])
const ID_PATTERN = /^[A-Za-z0-9_-]{1,64}$/

export function applyOperations(scene: CanvasScene, operations: CanvasOperation[]): CanvasScene {
  const next: CanvasScene = structuredClone(scene)
  for (const operation of operations) applyOperation(next, operation)
  validateScene(next)
  return next
}

function applyOperation(scene: CanvasScene, operation: CanvasOperation): void {
  switch (operation.type) {
    case "add_node": return addNode(scene, operation.node)
    case "update_node": return updateNode(scene, operation.nodeId, operation.patch)
    case "remove_node": return removeNode(scene, operation.nodeId)
    case "add_connection": return addConnection(scene, operation.connection)
    case "remove_connection": return removeConnection(scene, operation.connectionId)
    case "set_viewport": return setViewport(scene, operation.viewport)
    default: throw invalid("operation_type_invalid")
  }
}

function addNode(scene: CanvasScene, node: CanvasNodeData): void {
  validateNode(node)
  if (scene.nodes.some((item) => item.id === node.id)) throw invalid("node_id_duplicate")
  scene.nodes.push(structuredClone(node))
}

function updateNode(scene: CanvasScene, nodeId: string, patch: Record<string, unknown>): void {
  const node = scene.nodes.find((item) => item.id === nodeId)
  if (!node) throw invalid("node_not_found")
  if (Object.keys(patch).some((key) => FORBIDDEN_KEYS.has(key))) throw invalid("node_patch_key_invalid")
  Object.assign(node, structuredClone(patch))
  validateNode(node)
}

function removeNode(scene: CanvasScene, nodeId: string): void {
  if (!scene.nodes.some((item) => item.id === nodeId)) throw invalid("node_not_found")
  scene.nodes = scene.nodes.filter((item) => item.id !== nodeId)
  scene.connections = scene.connections.filter((item) => item.fromNodeId !== nodeId && item.toNodeId !== nodeId)
}

function addConnection(scene: CanvasScene, connection: CanvasConnection): void {
  validateConnection(connection)
  if (scene.connections.some((item) => item.id === connection.id)) throw invalid("connection_id_duplicate")
  if (!scene.nodes.some((item) => item.id === connection.fromNodeId) || !scene.nodes.some((item) => item.id === connection.toNodeId)) throw invalid("connection_target_missing")
  scene.connections.push(structuredClone(connection))
}

function removeConnection(scene: CanvasScene, connectionId: string): void {
  if (!scene.connections.some((item) => item.id === connectionId)) throw invalid("connection_not_found")
  scene.connections = scene.connections.filter((item) => item.id !== connectionId)
}

function setViewport(scene: CanvasScene, viewport: CanvasScene["viewport"]): void {
  if (![viewport.x, viewport.y, viewport.k].every(Number.isFinite) || viewport.k < 0.05 || viewport.k > 5) throw invalid("viewport_invalid")
  scene.viewport = { ...viewport }
}

export function validateScene(scene: CanvasScene): void {
  if (scene.schemaVersion !== 1 || !ID_PATTERN.test(scene.canvasId) || !Number.isInteger(scene.revision) || scene.revision < 0 || !["dots", "lines", "blank"].includes(scene.backgroundMode) || typeof scene.showImageInfo !== "boolean" || Number.isNaN(Date.parse(scene.updatedAt))) throw new CanvasRuntimeError("scene_invalid", "scene identity is invalid")
  if (!Array.isArray(scene.nodes) || scene.nodes.length > MAX_SCENE_NODES || !Array.isArray(scene.connections) || scene.connections.length > MAX_SCENE_CONNECTIONS) throw new CanvasRuntimeError("scene_invalid", "scene collections are invalid")
  const nodeIds = new Set<string>()
  scene.nodes.forEach((node) => { validateNode(node); if (nodeIds.has(node.id)) throw invalid("node_id_duplicate"); nodeIds.add(node.id) })
  const connectionIds = new Set<string>()
  scene.connections.forEach((connection) => { validateConnection(connection); if (connectionIds.has(connection.id)) throw invalid("connection_id_duplicate"); if (!nodeIds.has(connection.fromNodeId) || !nodeIds.has(connection.toNodeId)) throw invalid("connection_target_missing"); connectionIds.add(connection.id) })
  setViewport(scene, scene.viewport)
}

function validateNode(node: CanvasNodeData): void {
  if (!node || !ID_PATTERN.test(node.id) || typeof node.type !== "string" || !node.type || node.type.length > 80 || ![node.x, node.y, node.width, node.height].every(Number.isFinite) || node.width <= 0 || node.height <= 0 || (node.rotation !== undefined && !Number.isFinite(node.rotation))) throw invalid("node_invalid")
  if (node.metadata !== undefined && (!node.metadata || typeof node.metadata !== "object" || Array.isArray(node.metadata) || Buffer.byteLength(JSON.stringify(node.metadata), "utf8") > MAX_NODE_METADATA_BYTES || containsForbiddenKey(node.metadata))) throw invalid("node_metadata_invalid")
}

function validateConnection(connection: CanvasConnection): void {
  if (!connection || !ID_PATTERN.test(connection.id) || !ID_PATTERN.test(connection.fromNodeId) || !ID_PATTERN.test(connection.toNodeId)) throw invalid("connection_invalid")
}

function containsForbiddenKey(value: unknown): boolean {
  if (!value || typeof value !== "object") return false
  if (Array.isArray(value)) return value.some(containsForbiddenKey)
  return Object.entries(value).some(([key, item]) => FORBIDDEN_KEYS.has(key) || containsForbiddenKey(item))
}
