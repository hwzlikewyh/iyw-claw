import { LoadingManager } from "three"
import { GLTFLoader, type GLTF } from "three/addons/loaders/GLTFLoader.js"
import { DRACOLoader } from "three/addons/loaders/DRACOLoader.js"
import { MeshoptDecoder } from "three/addons/libs/meshopt_decoder.module.js"
import { disposeModel } from "./model-scene"

const MAX_MODEL_BYTES = 128 * 1024 * 1024
const GLB_MAGIC = 0x46546c67
const JSON_CHUNK = 0x4e4f534a

export function createModelLoader(src: string) {
  const manager = new LoadingManager()
  manager.setURLModifier((url) => {
    if (/^(data:|blob:)/.test(url)) return url
    if (!src.includes("/preview_resource/")) return new URL(url, src).href
    return `${src.slice(0, src.lastIndexOf("/") + 1)}${encodeURIComponent(url)}`
  })
  const draco = new DRACOLoader()
    .setDecoderPath("/preview-assets/three/draco/gltf/")
    .setWorkerLimit(1)
  const loader = new GLTFLoader(manager)
    .setDRACOLoader(draco)
    .setMeshoptDecoder(MeshoptDecoder)
  return {
    async load(signal: AbortSignal): Promise<GLTF> {
      const bytes = await readModel(src, signal)
      const json = inspectModel(bytes)
      validateModel(json)
      if (signal.aborted) throw new DOMException("Aborted", "AbortError")
      const model = await loader.parseAsync(bytes, "")
      if (signal.aborted) {
        for (const scene of model.scenes) disposeModel(scene)
        throw new DOMException("Aborted", "AbortError")
      }
      return model
    },
    dispose() {
      manager.abort()
      draco.dispose()
    },
  }
}

async function readModel(
  src: string,
  signal: AbortSignal
): Promise<ArrayBuffer> {
  const response = await fetch(src, { signal, credentials: "omit" })
  if (!response.ok) throw new Error("Model request failed")
  if (Number(response.headers.get("content-length")) > MAX_MODEL_BYTES) {
    await response.body?.cancel()
    throw new Error("Model exceeds preview limit")
  }
  const reader = response.body?.getReader()
  if (!reader) throw new Error("Model response has no body")
  const chunks: Uint8Array[] = []
  let size = 0
  try {
    while (true) {
      const { value, done } = await reader.read()
      if (done) break
      size += value.byteLength
      if (size > MAX_MODEL_BYTES) throw new Error("Model exceeds preview limit")
      chunks.push(value)
    }
    const bytes = new Uint8Array(size)
    let offset = 0
    for (const chunk of chunks) {
      bytes.set(chunk, offset)
      offset += chunk.length
    }
    return bytes.buffer
  } finally {
    await reader.cancel().catch(() => {})
    reader.releaseLock()
  }
}

interface ModelManifest {
  buffers?: Array<{ byteLength?: number }>
  extensionsRequired?: string[]
}

function inspectModel(bytes: ArrayBuffer): ModelManifest {
  const view = new DataView(bytes)
  if (view.byteLength >= 20 && view.getUint32(0, true) === GLB_MAGIC) {
    const length = view.getUint32(12, true)
    if (
      view.getUint32(16, true) !== JSON_CHUNK ||
      length > bytes.byteLength - 20
    )
      throw new Error("Invalid GLB header")
    return JSON.parse(
      new TextDecoder().decode(new Uint8Array(bytes, 20, length))
    )
  }
  return JSON.parse(new TextDecoder().decode(bytes))
}

function validateModel(manifest: ModelManifest) {
  const size = (manifest.buffers ?? []).reduce(
    (total, buffer) => total + (buffer.byteLength ?? 0),
    0
  )
  if (!Number.isFinite(size) || size > MAX_MODEL_BYTES)
    throw new Error("Model buffers exceed preview limit")
  if (manifest.extensionsRequired?.includes("KHR_texture_basisu"))
    throw new Error("Compressed textures are not supported by this preview")
}
