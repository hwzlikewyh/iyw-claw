interface PreparePickedAttachmentPathsOptions {
  stageInChatDirectory: boolean
  chatDirectory?: string
  stage: (sourcePath: string, chatDirectory: string) => Promise<string>
}

export async function preparePickedAttachmentPaths(
  paths: string[],
  options: PreparePickedAttachmentPathsOptions
): Promise<string[]> {
  if (!options.stageInChatDirectory) return paths
  const chatDirectory = options.chatDirectory?.trim()
  if (!chatDirectory) {
    throw new Error("Chat working directory is unavailable")
  }
  const staged: string[] = []
  for (const sourcePath of paths) {
    staged.push(await options.stage(sourcePath, chatDirectory))
  }
  return staged
}
