export const IMAGE_DOWNLOAD_ORIGINS = ["https://vol-ai.iywtu.com"] as const
export const MAX_IMAGE_DOWNLOAD_BYTES = 32 * 1024 * 1024

export function imageDownloadUrl(value: string): URL {
  const url = new URL(value)
  if (!IMAGE_DOWNLOAD_ORIGINS.some((origin) => origin === url.origin) || url.username || url.password || url.hash) {
    throw new Error("图片地址不在已授权的素材来源中")
  }
  return url
}
