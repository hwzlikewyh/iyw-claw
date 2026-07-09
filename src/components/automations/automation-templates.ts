import {
  FlaskConical,
  ListChecks,
  Package,
  ScanSearch,
  ShieldCheck,
  Tag,
  Wrench,
  type LucideIcon,
} from "lucide-react"
import type {
  AgentType,
  AutomationDraft,
  AutomationIsolation,
  AutomationTriggerKind,
} from "@/lib/types"

/** i18n keys live in the `Automations` namespace; the unions keep `t(...)`
 *  type-checked against the typed message catalog. */
type TemplateTitleKey =
  | "tplCodeReviewTitle"
  | "tplDependencyUpdatesTitle"
  | "tplTestCoverageTitle"
  | "tplTodoSweepTitle"
  | "tplCiTriageTitle"
  | "tplReleaseNotesTitle"
  | "tplSecurityAuditTitle"

type TemplateDescKey =
  | "tplCodeReviewDesc"
  | "tplDependencyUpdatesDesc"
  | "tplTestCoverageDesc"
  | "tplTodoSweepDesc"
  | "tplCiTriageDesc"
  | "tplReleaseNotesDesc"
  | "tplSecurityAuditDesc"

export interface AutomationTemplate {
  id: string
  icon: LucideIcon
  /** Icon-chip classes: a text color + a matching low-alpha background tint. */
  accent: string
  titleKey: TemplateTitleKey
  descKey: TemplateDescKey
  /** Canonical English starting prompt; the user edits it before saving. Kept
   *  out of the i18n catalog deliberately — agent prompts are conventionally
   *  English and this is editable seed content, not chrome. */
  prompt: string
  trigger_kind: AutomationTriggerKind
  /** Suggested cadence. Carried even for manual templates so flipping the
   *  trigger to "schedule" in the editor keeps a sensible default. */
  cron: string
  isolation: AutomationIsolation
}

export const AUTOMATION_TEMPLATES: AutomationTemplate[] = [
  {
    id: "product-detail-design-review",
    icon: ScanSearch,
    accent: "text-blue-500 bg-blue-500/10",
    titleKey: "tplCodeReviewTitle",
    descKey: "tplCodeReviewDesc",
    prompt:
      "Review the current product detail page from an ecommerce design perspective. Focus on first-screen clarity, product imagery, price and promotion visibility, trust signals, variant selection, primary call to action, and mobile readability. Produce concrete UI and copy recommendations; do not change any files.",
    trigger_kind: "schedule",
    cron: "0 9 * * 1-5",
    isolation: "worktree_per_run",
  },
  {
    id: "campaign-landing-page",
    icon: Package,
    accent: "text-amber-500 bg-amber-500/10",
    titleKey: "tplDependencyUpdatesTitle",
    descKey: "tplDependencyUpdatesDesc",
    prompt:
      "Create recommendations for an ecommerce campaign landing page. Cover hero layout, offer hierarchy, product grouping, urgency cues, coupon presentation, conversion path, and responsive behavior. Include a concise section-by-section structure and implementation notes for the design team.",
    trigger_kind: "schedule",
    cron: "0 9 * * 1",
    isolation: "worktree_per_run",
  },
  {
    id: "checkout-experience-audit",
    icon: FlaskConical,
    accent: "text-emerald-500 bg-emerald-500/10",
    titleKey: "tplTestCoverageTitle",
    descKey: "tplTestCoverageDesc",
    prompt:
      "Audit the ecommerce checkout experience. Review cart clarity, shipping and payment steps, error states, trust cues, form friction, discount-code handling, order summary visibility, and mobile flow. List the highest-impact improvements first with expected conversion impact.",
    trigger_kind: "schedule",
    cron: "0 9 * * 1",
    isolation: "worktree_per_run",
  },
  {
    id: "design-system-sweep",
    icon: ListChecks,
    accent: "text-violet-500 bg-violet-500/10",
    titleKey: "tplTodoSweepTitle",
    descKey: "tplTodoSweepDesc",
    prompt:
      "Review the interface for design-system consistency. Check typography scale, spacing, button hierarchy, form controls, cards, color usage, empty states, and repeated ecommerce components. Produce a prioritized list of consistency fixes; do not change any files.",
    trigger_kind: "manual",
    cron: "0 9 * * 1",
    isolation: "worktree_per_run",
  },
  {
    id: "storefront-home-redesign",
    icon: Wrench,
    accent: "text-orange-500 bg-orange-500/10",
    titleKey: "tplCiTriageTitle",
    descKey: "tplCiTriageDesc",
    prompt:
      "Propose a storefront homepage redesign for an ecommerce site. Define the content hierarchy from top to bottom, including brand promise, category navigation, featured products, promotional modules, social proof, and retention entry points. Keep the recommendations practical for implementation.",
    trigger_kind: "manual",
    cron: "0 * * * *",
    isolation: "worktree_per_run",
  },
  {
    id: "product-selling-points",
    icon: Tag,
    accent: "text-sky-500 bg-sky-500/10",
    titleKey: "tplReleaseNotesTitle",
    descKey: "tplReleaseNotesDesc",
    prompt:
      "Improve ecommerce product selling points and page copy. Review the current title, subtitle, benefit bullets, feature explanations, FAQ, and CTA microcopy. Rewrite the key sections in a clearer, conversion-focused style and explain the rationale for each change.",
    trigger_kind: "manual",
    cron: "0 9 * * 1",
    isolation: "worktree_per_run",
  },
  {
    id: "competitor-commerce-analysis",
    icon: ShieldCheck,
    accent: "text-rose-500 bg-rose-500/10",
    titleKey: "tplSecurityAuditTitle",
    descKey: "tplSecurityAuditDesc",
    prompt:
      "Analyze competing ecommerce pages in this category. Compare layout structure, product presentation, offer framing, credibility signals, checkout entry points, and mobile experience. Summarize patterns worth adopting and risks to avoid; do not change any files.",
    trigger_kind: "schedule",
    cron: "0 9 * * 1",
    isolation: "worktree_per_run",
  },
]

function detectTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC"
  } catch {
    return "UTC"
  }
}

/** Build an editor seed draft from a template. The localized `name` is resolved
 *  by the caller (it lives in the i18n catalog); agent + folder come from the
 *  workspace defaults. */
export function templateToDraft(
  template: AutomationTemplate,
  opts: { name: string; agentType: AgentType; folderId: number | null }
): AutomationDraft {
  return {
    name: opts.name,
    enabled: true,
    trigger_kind: template.trigger_kind,
    cron: template.cron,
    timezone: detectTimezone(),
    agent_type: opts.agentType,
    root_folder_id: opts.folderId,
    isolation: template.isolation,
    branch: null,
    is_remote_branch: false,
    config: {
      prompt_blocks: [{ type: "text", text: template.prompt }],
      display_text: template.prompt,
      mode_id: null,
      config_values: {},
    },
  }
}
