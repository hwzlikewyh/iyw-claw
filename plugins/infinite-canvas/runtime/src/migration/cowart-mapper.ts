import { createHash } from "node:crypto"
import type { CanvasConnection, CanvasNodeData, CanvasScene } from "../types.js"
import type { CowartPage, CowartRecord, MigrationUnsupportedRecord } from "./cowart-types.js"

type Mapped = { node: CanvasNodeData; source: CowartRecord }
type Mapping = { scene: CanvasScene; mapped: number; skipped: number; warnings: string[]; unsupportedRecords: MigrationUnsupportedRecord[] }

export function mapCowartPage(page: CowartPage, targetCanvasId: string): Mapping {
  const warnings = [...page.warnings]
  const unsupportedRecords: MigrationUnsupportedRecord[] = []
  const mapped: Mapped[] = []
  for (const record of page.records) {
    const node = mapRecord(record, targetCanvasId, page.pageId)
    if (node) mapped.push({ node, source: record })
    else {
      const type = record.typeName ?? record.type ?? "unknown"
      unsupportedRecords.push({ id: record.id, type, reason: "unsupported_record_type" })
      warnings.push(`record_${record.id}_unsupported`)
    }
  }
  const nodes = mapped.map((item) => item.node)
  const connections = mapConnections(mapped, nodes, warnings)
  return { scene: createScene(targetCanvasId, nodes, connections), mapped: nodes.length, skipped: page.records.length - nodes.length, warnings, unsupportedRecords }
}

function mapRecord(record: CowartRecord, canvasId: string, pageId: string): CanvasNodeData | null {
  const kind = (record.typeName ?? record.type ?? "").toLowerCase()
  const props = record.props ?? {}
  const id = stableId(canvasId, record.id)
  const position = { x: record.x ?? numberProp(props, "x", 0), y: record.y ?? numberProp(props, "y", 0) }
  const size = { width: Math.max(1, numberProp(props, "w", numberProp(props, "width", 240))), height: Math.max(1, numberProp(props, "h", numberProp(props, "height", 160))) }
  const sourcePath = assetPath(record, pageId)
  const base = { id, ...position, ...size, rotation: record.rotation ?? 0, metadata: { ...safeMetadata(record), ...(sourcePath ? { sourcePath } : {}) } }
  if (kind.includes("cowartaiimageholder")) return { ...base, type: "image-config", metadata: { ...base.metadata, status: statusValue(props.status), prompt: stringValue(props.prompt) } }
  if (kind.includes("image") || kind.includes("video")) return { ...base, type: "media" }
  if (kind.includes("text") || kind.includes("note")) return { ...base, type: "text" }
  if (kind.includes("markdown")) return { ...base, type: "markdown:doc", metadata: { ...base.metadata, content: stringValue(props.content) ?? stringValue(props.text) ?? "" } }
  if (kind.includes("svg")) return { ...base, type: "svg:vector", metadata: { ...base.metadata, content: stringValue(props.content) ?? stringValue(props.svg) ?? "" } }
  if (kind.includes("frame") || kind.includes("group")) return { ...base, type: "group" }
  if (kind.includes("cowartaidraftholder") || kind.includes("htmldraft") || kind.includes("html") || kind.includes("bookmark") || kind.includes("embed")) return { ...base, type: "iyw:html", metadata: { ...base.metadata, source: stringValue(props.html) ?? stringValue(props.content) ?? "" } }
  if (kind.includes("cowartaislides")) return { ...base, type: "iyw:slides", metadata: { ...base.metadata, deck: slideDeck(record, id) } }
  if (kind.includes("geo")) return { ...base, type: "iyw:annotation-layer", metadata: { ...base.metadata, shapes: [geoShape(record, id)] } }
  if (kind.includes("arrow") || kind.includes("line") || kind.includes("draw")) return { ...base, type: "iyw:annotation-layer", metadata: { ...base.metadata, shapes: [lineShape(record, id)] } }
  return null
}

function mapConnections(mapped: Mapped[], nodes: CanvasNodeData[], warnings: string[]): CanvasConnection[] {
  const ids = new Map(mapped.map(({ source, node }) => [source.id, node.id]))
  const connections: CanvasConnection[] = []
  for (const { source } of mapped) {
    const kind = (source.typeName ?? source.type ?? "").toLowerCase()
    if (!kind.includes("arrow") && !kind.includes("line")) continue
    const props = source.props ?? {}
    const from = bindingId(props, "startBinding") ?? bindingId(props, "from")
    const to = bindingId(props, "endBinding") ?? bindingId(props, "to")
    const fromNodeId = from ? ids.get(from) : undefined
    const toNodeId = to ? ids.get(to) : undefined
    if (fromNodeId && toNodeId && fromNodeId !== toNodeId) connections.push({ id: stableId("connection", source.id), fromNodeId, toNodeId })
    else if (from || to) warnings.push(`record_${source.id}_binding_unresolved`)
  }
  return connections.filter((connection, index) => connections.findIndex((item) => item.id === connection.id) === index && nodes.some((node) => node.id === connection.fromNodeId) && nodes.some((node) => node.id === connection.toNodeId))
}

function createScene(canvasId: string, nodes: CanvasNodeData[], connections: CanvasConnection[]): CanvasScene {
  return { schemaVersion: 1, canvasId, revision: 0, nodes, connections, backgroundMode: "dots", showImageInfo: true, viewport: { x: 0, y: 0, k: 1 }, updatedAt: new Date(0).toISOString() }
}

function lineShape(record: CowartRecord, id: string) {
  const props = record.props ?? {}
  const from = pointValue(props.start) ?? { x: 0, y: 0 }
  const to = pointValue(props.end) ?? { x: numberProp(props, "w", 1), y: numberProp(props, "h", 1) }
  return { id: `shape-${id}`, type: "arrow" as const, from: normalizePoint(from), to: normalizePoint(to), color: colorValue(props.color) }
}

function geoShape(record: CowartRecord, id: string) {
  const props = record.props ?? {}
  const shapeType = String(props.geo ?? props.shape ?? "rect").toLowerCase() === "ellipse" ? "ellipse" as const : "rect" as const
  return { id: `shape-${id}`, type: shapeType, x: 0, y: 0, width: 1, height: 1, color: colorValue(props.color) }
}

function safeMetadata(record: CowartRecord): Record<string, unknown> {
  return { cowartRecordId: record.id, cowartType: record.typeName ?? record.type ?? "unknown", parentId: record.parentId, props: compactObject(record.props), meta: compactObject(record.meta) }
}

function slideDeck(record: CowartRecord, id: string) {
  const props = record.props ?? {}
  const title = stringValue(props.title) ?? "Imported slides"
  return { schemaVersion: 1 as const, title, theme: { background: "#101318", foreground: "#f4f7fb", accent: "#72b7ff" }, pages: [{ id: `page-${id}`, title, html: stringValue(props.html) ?? "<p>Imported slide</p>", notes: stringValue(props.notes) ?? "" }], activePageId: `page-${id}` }
}

function compactObject(value: Record<string, unknown> | undefined): Record<string, unknown> {
  if (!value) return {}
  return Object.fromEntries(Object.entries(value).filter(([key, item]) => !["__proto__", "prototype", "constructor"].includes(key) && (typeof item !== "string" || item.length <= 10000) && (typeof item !== "object" || item === null)))
}

function assetPath(record: CowartRecord, pageId: string): string | undefined {
  const props = record.props ?? {}
  const value = [props.assetPath, props.src, props.fileName].find((item): item is string => typeof item === "string" && item.trim().length > 0)
  if (!value) return undefined
  const clean = value.trim().replaceAll("\\", "/")
  if (clean.startsWith("/") || clean.includes(":") || clean.split("/").some((part) => !part || part === "." || part === "..")) return undefined
  return `canvas/pages/${pageId}/${clean}`
}

function bindingId(props: Record<string, unknown>, key: string): string | undefined {
  const value = props[key]
  if (typeof value === "string") return value
  if (value && typeof value === "object" && typeof (value as Record<string, unknown>).boundShapeId === "string") return (value as Record<string, unknown>).boundShapeId as string
  if (value && typeof value === "object" && typeof (value as Record<string, unknown>).id === "string") return (value as Record<string, unknown>).id as string
  return undefined
}

function pointValue(value: unknown): { x: number; y: number } | undefined {
  if (!value || typeof value !== "object") return undefined
  const point = value as Record<string, unknown>
  return typeof point.x === "number" && typeof point.y === "number" && Number.isFinite(point.x) && Number.isFinite(point.y) ? { x: point.x, y: point.y } : undefined
}

function normalizePoint(point: { x: number; y: number }) { return { x: Math.max(0, Math.min(1, point.x)), y: Math.max(0, Math.min(1, point.y)) } }
function colorValue(value: unknown): string { return typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value) ? value : "#72b7ff" }
function stringValue(value: unknown): string | undefined { return typeof value === "string" && value.length <= 200000 ? value : undefined }
function statusValue(value: unknown): "pending" | "error" { return value === "error" ? "error" : "pending" }
function stableId(scope: string, source: string): string { return `${scope}-${createHash("sha256").update(`${scope}:${source}`).digest("hex").slice(0, 16)}` }
function numberProp(props: Record<string, unknown>, key: string, fallback: number): number { return typeof props[key] === "number" && Number.isFinite(props[key]) ? props[key] as number : fallback }
