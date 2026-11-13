// 沙箱的 connect-src 不允许 fetch(blob:)，本页创建的素材直接从内存读取。
export function installBlobFetch(): () => void {
  const blobs = new Map<string, Blob>()
  const create = URL.createObjectURL.bind(URL)
  const revoke = URL.revokeObjectURL.bind(URL)
  const fetch = window.fetch.bind(window)
  URL.createObjectURL = (value) => {
    const url = create(value)
    if (value instanceof Blob) blobs.set(url, value)
    return url
  }
  URL.revokeObjectURL = (url) => { blobs.delete(url); revoke(url) }
  window.fetch = async (input, init) => {
    const url = input instanceof Request ? input.url : String(input)
    const signal = init?.signal ?? (input instanceof Request ? input.signal : undefined)
    if (signal?.aborted) throw new DOMException("Aborted", "AbortError")
    const blob = blobs.get(url)
    if (blob) return new Response(blob)
    if (url.startsWith("data:")) return dataResponse(url)
    return fetch(input, init)
  }
  return () => {
    for (const url of blobs.keys()) revoke(url)
    blobs.clear()
  }
}

function dataResponse(url: string): Response {
  const comma = url.indexOf(",")
  if (comma < 0) throw new TypeError("Invalid data URL")
  const header = url.slice(5, comma)
  const content = decodeURIComponent(url.slice(comma + 1))
  const body = header.endsWith(";base64")
    ? Uint8Array.from(atob(content), (character) => character.charCodeAt(0))
    : new TextEncoder().encode(content)
  return new Response(body, { headers: { "content-type": header.replace(/;base64$/, "") || "text/plain;charset=US-ASCII" } })
}
