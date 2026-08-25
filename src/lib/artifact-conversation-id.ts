export function resolveArtifactConversationId(
  runtimeConversationId: number,
  persistedConversationId?: number | null
): number | null {
  return persistedConversationId === undefined
    ? runtimeConversationId
    : persistedConversationId
}
