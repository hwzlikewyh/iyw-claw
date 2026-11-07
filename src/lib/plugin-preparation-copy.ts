const ISSUE_KEYS = {
  dependency_cycle_or_limit: "issues.dependency_cycle_or_limit",
  dependency_snapshot_missing: "issues.dependency_snapshot_missing",
  dependency_unavailable: "issues.dependency_unavailable",
  dependency_permission_required: "issues.dependency_permission_required",
  dependency_disabled: "issues.dependency_disabled",
  dependency_configuration_required: "issues.dependency_configuration_required",
  plugin_unavailable: "issues.plugin_unavailable",
  agent_unsupported: "issues.agent_unsupported",
} as const

export function pluginPreparationIssueKey(issue: string) {
  return (
    ISSUE_KEYS[issue as keyof typeof ISSUE_KEYS] || "issues.plugin_unavailable"
  )
}
