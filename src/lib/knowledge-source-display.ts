const KNOWLEDGE_SOURCE_URI =
  /^iyw-knowledge:\/\/source\/[a-f0-9]{64}\/[01]\/(file|folder)\/\d{1,128}\/\d{1,128}$/i

export function knowledgeSourceKind(uri: string): "file" | "folder" | null {
  const kind = KNOWLEDGE_SOURCE_URI.exec(uri)?.[1]?.toLowerCase()
  return kind === "file" || kind === "folder" ? kind : null
}

// 星河历史记录会将资源块保存为名称和 URI 两行文本。
export function extractSerializedKnowledgeSources(text: string) {
  const resources: { name: string; uri: string }[] = []
  const visibleText = text.replace(
    /^Referenced resource: ([^\r\n]+)\r?\nURI: (iyw-knowledge:\/\/source\/[^\s]+)[\t ]*(?:\r?\nUser-attached knowledge source for this message\.[^\r\n]*)?/gm,
    (match: string, name: string, uri: string) => {
      if (!knowledgeSourceKind(uri)) return match
      resources.push({ name: name.trim(), uri })
      return ""
    }
  )
  return { text: visibleText, resources }
}
