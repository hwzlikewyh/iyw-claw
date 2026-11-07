export function environmentNames(spec) {
  if (spec.env_vars !== undefined && !Array.isArray(spec.env_vars))
    throw Error("Invalid native env_vars")
  const references = Object.values(spec.env ?? {})
    .flatMap((value) =>
      [...value.matchAll(/\$\{([A-Za-z_][A-Za-z0-9_]*)/g)].map(
        (match) => match[1]
      )
    )
    .filter(
      (name) => !["CLAUDE_PLUGIN_ROOT", "CODEX_PLUGIN_ROOT"].includes(name)
    )
  const envVars = [...new Set([...(spec.env_vars ?? []), ...references])]
  if (
    envVars.some(
      (key) =>
        typeof key !== "string" ||
        !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key) ||
        /^(IYW_|CODEX_|CLAUDE_|OPENAI_|ANTHROPIC_|AWS_|AZURE_)/i.test(key) ||
        /^(NODE_OPTIONS|NODE_PATH|LD_PRELOAD|DYLD_INSERT_LIBRARIES)$/i.test(key)
    )
  )
    throw Error("Invalid native env_vars")
  return envVars
}
