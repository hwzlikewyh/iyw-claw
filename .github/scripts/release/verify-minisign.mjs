import { createReadStream } from "node:fs"
import { createHash, createPublicKey, timingSafeEqual, verify } from "node:crypto"

// 沿用 payload_verify.cpp 的固定密钥、预哈希和附注验证，不依赖目标系统另装 CLI。
export async function verifyMinisign(path, signatureText, publicKeyText) {
  const text = signatureText.startsWith("untrusted comment:") ? signatureText :
    Buffer.from(signatureText.trim(), "base64").toString("utf8")
  const [, encoded, trusted, global] = text.trim().split(/\r?\n/)
  if (!encoded || !trusted?.startsWith("trusted comment: ") || !global)
    throw new Error("invalid minisign signature envelope")
  const key = Buffer.from(publicKeyText, "base64")
  const signature = Buffer.from(encoded, "base64")
  const commentSignature = Buffer.from(global, "base64")
  if (key.length !== 42 || signature.length !== 74 || commentSignature.length !== 64 ||
      signature.subarray(0, 2).toString() !== "ED" ||
      !timingSafeEqual(key.subarray(2, 10), signature.subarray(2, 10)))
    throw new Error("minisign key or signature mismatch")
  const publicKey = createPublicKey({ key: Buffer.concat([
    Buffer.from("302a300506032b6570032100", "hex"), key.subarray(10),
  ]), format: "der", type: "spki" })
  const hash = createHash("blake2b512")
  for await (const chunk of createReadStream(path)) hash.update(chunk)
  if (!verify(null, hash.digest(), publicKey, signature.subarray(10)) ||
      !verify(null, Buffer.concat([signature.subarray(10), Buffer.from(trusted.slice(17))]), publicKey, commentSignature))
    throw new Error("minisign release signature verification failed")
}
