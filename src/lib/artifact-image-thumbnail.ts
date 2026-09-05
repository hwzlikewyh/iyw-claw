const THUMBNAIL_WIDTH = 200
const QINIU_HOST_SUFFIXES = [
  ".clouddn.com",
  ".qbox.me",
  ".qiniucdn.com",
  ".qnssl.com",
]

export function buildArtifactThumbnailUrl(source: string): string {
  const url = parsePublicImageUrl(source)
  if (!url || url.search || url.hash) return source
  const host = url.hostname.toLowerCase()
  if (isTosHost(host)) {
    return withQuery(url, "x-tos-process", resizeCommand())
  }
  if (isOssHost(host)) {
    return withQuery(url, "x-oss-process", resizeCommand())
  }
  if (matchesHostSuffix(host, ".bcebos.com")) {
    return withQuery(
      url,
      "x-bce-process",
      `image/resize,m_lfit,w_${THUMBNAIL_WIDTH},h_${THUMBNAIL_WIDTH}`
    )
  }
  if (QINIU_HOST_SUFFIXES.some((suffix) => matchesHostSuffix(host, suffix))) {
    return `${url.toString()}?imageMogr2/thumbnail/!${THUMBNAIL_WIDTH}x${THUMBNAIL_WIDTH}r`
  }
  return source
}

function parsePublicImageUrl(source: string): URL | null {
  try {
    const url = new URL(source)
    if (
      url.protocol !== "https:" ||
      url.username.length > 0 ||
      url.password.length > 0
    ) {
      return null
    }
    return url
  } catch {
    return null
  }
}

function isTosHost(host: string): boolean {
  return (
    (host.endsWith(".volces.com") || host.endsWith(".ivolces.com")) &&
    (host.startsWith("tos-") || host.includes(".tos-"))
  )
}

function isOssHost(host: string): boolean {
  return (
    matchesHostSuffix(host, ".aliyuncs.com") &&
    (host.startsWith("oss-") || host.includes(".oss-"))
  )
}

function resizeCommand(): string {
  return `image/resize,w_${THUMBNAIL_WIDTH}`
}

function withQuery(url: URL, key: string, value: string): string {
  return `${url.toString()}?${key}=${value}`
}

function matchesHostSuffix(host: string, suffix: string): boolean {
  return host === suffix.slice(1) || host.endsWith(suffix)
}
