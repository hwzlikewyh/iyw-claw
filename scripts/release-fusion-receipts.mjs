import { readFileSync, statSync } from "node:fs"
import { join } from "node:path"

const MAX_RECEIPT_BYTES = 128 * 1024

export async function submitArtifactReceipt(api, artifact, input) {
  const { id, releaseId } = artifact
  if (![id, releaseId].every(value => typeof value === "string" && /^[1-9]\d*$/.test(value)))
    throw new Error("invalid Fusion artifact identity")
  if (!process.env.FUSION_RECEIPT_DIR) return null
  const path = join(process.env.FUSION_RECEIPT_DIR, `${id}.json`)
  let receipt
  try {
    const file = statSync(path)
    if (!file.isFile() || file.size > MAX_RECEIPT_BYTES) throw new Error(`invalid receipt file for ${id}`)
    receipt = JSON.parse(readFileSync(path, "utf8"))
  } catch (error) {
    if (error.code === "ENOENT") return null
    throw error
  }
  const claims = receipt?.claims
  if (claims?.artifactId !== id || claims.releaseId !== releaseId || claims.operation !== "complete" ||
      claims.product !== "iyw-claw" || claims.size !== input.size || claims.sha256 !== input.sha256 ||
      claims.updaterSignature !== input.signature || !receipt.signature)
    throw new Error(`verifier receipt differs from signed source for ${input.fileName}`)
  const result = await api(`/admin/api/app-releases/${releaseId}/artifacts/${id}/verified-complete`, { receipt })
  if (result.artifact?.id !== id || result.artifact.releaseId !== releaseId)
    throw new Error(`Fusion returned a different artifact for ${id}`)
  return result.artifact
}
