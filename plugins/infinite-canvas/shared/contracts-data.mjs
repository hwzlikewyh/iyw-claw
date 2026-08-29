const schema = (properties, required = []) => ({
  $schema: "http://json-schema.org/draft-07/schema#",
  type: "object",
  properties,
  ...(required.length ? { required } : {}),
  additionalProperties: false,
})

const id = { type: "string", pattern: "^[A-Za-z0-9_-]{1,64}$" }
const sha256 = { type: "string", pattern: "^[a-f0-9]{64}$" }
const viewport = {
  type: "object",
  properties: {
    x: { type: "number" },
    y: { type: "number" },
    k: { type: "number", minimum: 0.05, maximum: 5 },
  },
  required: ["x", "y", "k"],
  additionalProperties: false,
}
const node = {
  type: "object",
  properties: {
    id,
    type: { type: "string", minLength: 1, maxLength: 80 },
    x: { type: "number" },
    y: { type: "number" },
    width: { type: "number", exclusiveMinimum: 0 },
    height: { type: "number", exclusiveMinimum: 0 },
    rotation: { type: "number" },
    metadata: { type: "object", additionalProperties: true },
  },
  required: ["id", "type", "x", "y", "width", "height"],
  additionalProperties: true,
}
const connection = {
  type: "object",
  properties: {
    id,
    fromNodeId: id,
    toNodeId: id,
  },
  required: ["id", "fromNodeId", "toNodeId"],
  additionalProperties: false,
}
const operation = {
  oneOf: [
    { type: "object", properties: { type: { const: "add_node" }, node }, required: ["type", "node"], additionalProperties: false },
    { type: "object", properties: { type: { const: "update_node" }, nodeId: id, patch: { type: "object", additionalProperties: true } }, required: ["type", "nodeId", "patch"], additionalProperties: false },
    { type: "object", properties: { type: { const: "remove_node" }, nodeId: id }, required: ["type", "nodeId"], additionalProperties: false },
    { type: "object", properties: { type: { const: "add_connection" }, connection }, required: ["type", "connection"], additionalProperties: false },
    { type: "object", properties: { type: { const: "remove_connection" }, connectionId: id }, required: ["type", "connectionId"], additionalProperties: false },
    { type: "object", properties: { type: { const: "set_viewport" }, viewport }, required: ["type", "viewport"], additionalProperties: false },
  ],
}
const scene = {
  type: "object",
  properties: {
    schemaVersion: { const: 1 },
    canvasId: id,
    revision: { type: "integer", minimum: 0 },
    nodes: { type: "array", items: node, maxItems: 10000 },
    connections: { type: "array", items: connection, maxItems: 20000 },
    backgroundMode: { enum: ["dots", "lines", "blank"] },
    showImageInfo: { type: "boolean" },
    viewport,
    updatedAt: { type: "string", format: "date-time" },
  },
  required: ["schemaVersion", "canvasId", "revision", "nodes", "connections", "backgroundMode", "showImageInfo", "viewport", "updatedAt"],
  additionalProperties: false,
}

export const contracts = Object.freeze({
  render_infinite_canvas_widget: {
    schemaPath: "contracts/render-infinite-canvas-widget.schema.json",
    description: "Open the Infinite Canvas MCP App widget.",
    readOnlyHint: true,
    inputSchema: schema({ canvasId: id, displayMode: { enum: ["inline", "fullscreen"] } }),
  },
  get_infinite_canvas_state: {
    schemaPath: "contracts/get-infinite-canvas-state.schema.json",
    description: "Read the current project canvas scene.",
    readOnlyHint: true,
    inputSchema: schema({ canvasId: id, sinceRevision: { type: "integer", minimum: 0 } }),
  },
  get_infinite_canvas_selection: {
    schemaPath: "contracts/get-infinite-canvas-selection.schema.json",
    description: "Read the current canvas selection.",
    readOnlyHint: true,
    inputSchema: schema({ canvasId: id }, ["canvasId"]),
  },
  save_infinite_canvas_selection: {
    schemaPath: "contracts/save-infinite-canvas-selection.schema.json",
    description: "Persist the current canvas selection.",
    readOnlyHint: false,
    inputSchema: schema({ canvasId: id, revision: { type: "integer", minimum: 0 }, selectedNodeIds: { type: "array", items: id, maxItems: 200 } }, ["canvasId", "revision", "selectedNodeIds"]),
  },
  apply_infinite_canvas_ops: {
    schemaPath: "contracts/apply-infinite-canvas-ops.schema.json",
    description: "Apply ordered scene operations with revision protection.",
    readOnlyHint: false,
    inputSchema: schema({ canvasId: id, baseRevision: { type: "integer", minimum: 0 }, operations: { type: "array", items: operation, minItems: 1, maxItems: 200 } }, ["canvasId", "baseRevision", "operations"]),
  },
  save_infinite_canvas_snapshot: {
    schemaPath: "contracts/save-infinite-canvas-snapshot.schema.json",
    description: "Persist a complete canvas snapshot with revision protection.",
    readOnlyHint: false,
    inputSchema: schema({ canvasId: id, baseRevision: { type: "integer", minimum: 0 }, scene }, ["canvasId", "baseRevision", "scene"]),
  },
  read_infinite_canvas_asset: {
    schemaPath: "contracts/read-infinite-canvas-asset.schema.json",
    description: "Read a range from a content-addressed canvas asset.",
    readOnlyHint: true,
    inputSchema: schema({ sha256, offset: { type: "integer", minimum: 0 }, length: { type: "integer", minimum: 1, maximum: 131072 } }, ["sha256", "offset", "length"]),
  },
  write_infinite_canvas_asset: {
    schemaPath: "contracts/write-infinite-canvas-asset.schema.json",
    description: "Upload or import a canvas asset using verified chunks.",
    readOnlyHint: false,
    inputSchema: schema({ uploadId: id, sourcePath: { type: "string", minLength: 1, maxLength: 4096 }, name: { type: "string", minLength: 1, maxLength: 180 }, mimeType: { type: "string", minLength: 1, maxLength: 120 }, expectedBytes: { type: "integer", minimum: 1 }, expectedSha256: sha256, chunkIndex: { type: "integer", minimum: 0 }, dataBase64: { type: "string", minLength: 1, maxLength: 174768 }, finalize: { type: "boolean" } }),
  },
  export_infinite_canvas: {
    schemaPath: "contracts/export-infinite-canvas.schema.json",
    description: "Export a canvas scene or a previously uploaded asset.",
    readOnlyHint: false,
    inputSchema: schema({ canvasId: id, format: { enum: ["json", "html", "png", "svg"] }, sourceAssetSha256: sha256, fileName: { type: "string", minLength: 1, maxLength: 180 } }, ["canvasId", "format"]),
  },
  migrate_cowart_canvas: {
    schemaPath: "contracts/migrate-cowart-canvas.schema.json",
    description: "Preview or migrate a Cowart page into a new canvas.",
    readOnlyHint: false,
    inputSchema: schema({ pageId: id, targetCanvasId: id, dryRun: { type: "boolean" } }, ["pageId"]),
  },
})

export const resourceUri = "ui://widget/infinite-canvas/canvas.html"
