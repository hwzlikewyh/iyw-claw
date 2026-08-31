import { describe, expect, it } from "vitest"

import {
  inferScenarioVariables,
  missingScenarioVariables,
  scenarioTemplateToDoc,
} from "./scenario-template"

describe("scenario template", () => {
  const variables = [
    {
      key: "market",
      label: "目标市场",
      type: "select" as const,
      options: ["美国", "欧洲", "自定义"],
      required: true,
      allowCustom: true,
    },
    {
      key: "category",
      label: "品类",
      type: "input" as const,
      options: [],
      required: true,
      allowCustom: true,
    },
  ]

  it("turns configured placeholders into inline variable nodes", () => {
    const doc = scenarioTemplateToDoc(
      "目标市场={{market}}，品类={{category}}。",
      variables
    )
    const nodes = doc.content?.[0]?.content ?? []
    expect(
      nodes.filter((node) => node.type === "scenarioVariable")
    ).toHaveLength(2)
    expect(
      nodes.find((node) => node.type === "scenarioVariable")?.attrs
    ).toMatchObject({
      key: "market",
      label: "目标市场",
      type: "select",
    })
  })

  it("reports required variables until their inline values are filled", () => {
    const doc = scenarioTemplateToDoc("{{market}}/{{category}}", variables)
    expect(missingScenarioVariables(doc)).toEqual(["目标市场", "品类"])
    const nodes = doc.content?.[0]?.content ?? []
    const market = nodes.find((node) => node.attrs?.key === "market")
    const category = nodes.find((node) => node.attrs?.key === "category")
    if (market?.attrs) market.attrs.value = "美国"
    if (category?.attrs) category.attrs.value = "陶瓷餐具"
    expect(missingScenarioVariables(doc)).toEqual([])
  })

  it("infers editable fallback controls for an older catalog", () => {
    expect(
      inferScenarioVariables("分析 {{market}} 的 {{category}}")
    ).toMatchObject([
      { key: "market", label: "目标市场", type: "input" },
      { key: "category", label: "品类", type: "input" },
    ])
  })
})
