interface FolderParentLink {
  id: number
  parent_id?: number | null
}

export function resolveConversationFolderScope(
  rootFolderId: number,
  folders: FolderParentLink[]
): number[] {
  return [
    rootFolderId,
    ...folders
      .filter((folder) => folder.parent_id === rootFolderId)
      .map((folder) => folder.id),
  ]
}
