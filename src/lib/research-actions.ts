import {
  FlaskConical,
  Gauge,
  LineChart,
  ScanSearch,
  ScatterChart,
  Search,
  Sigma,
  Sparkles,
  TestTubes,
  type LucideIcon,
} from "lucide-react"

export interface ResearchAction {
  id: string
  skillId: string
  icon: LucideIcon
}

export const RESEARCH_ACTIONS: ResearchAction[] = [
  {
    id: "scientific-brainstorming",
    skillId: "scientific-brainstorming",
    icon: Sparkles,
  },
  {
    id: "hypothesis-generation",
    skillId: "hypothesis-generation",
    icon: FlaskConical,
  },
  {
    id: "experimental-design",
    skillId: "experimental-design",
    icon: TestTubes,
  },
  {
    id: "statistical-power",
    skillId: "statistical-power",
    icon: Gauge,
  },
  {
    id: "statistical-analysis",
    skillId: "statistical-analysis",
    icon: Sigma,
  },
  {
    id: "exploratory-data-analysis",
    skillId: "exploratory-data-analysis",
    icon: ScatterChart,
  },
  {
    id: "scientific-visualization",
    skillId: "scientific-visualization",
    icon: LineChart,
  },
  {
    id: "scientific-critical-thinking",
    skillId: "scientific-critical-thinking",
    icon: ScanSearch,
  },
  { id: "paper-lookup", skillId: "paper-lookup", icon: Search },
]
