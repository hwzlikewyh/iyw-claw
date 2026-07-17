import { describe, expect, it } from "vitest"

import { withSandboxCsp } from "@/lib/html-preview-inline"

describe("withSandboxCsp fragment navigation", () => {
  it("pins one about:srcdoc base after the CSP in trusted and strict modes", () => {
    for (const trusted of [false, true]) {
      const output = withSandboxCsp(
        "<html><head><title>x</title></head><body></body></html>",
        { trusted }
      )
      const document = new DOMParser().parseFromString(output, "text/html")
      const bases = document.querySelectorAll("base")

      expect(output).toContain("base-uri about:")
      expect(bases).toHaveLength(1)
      expect(bases[0]?.getAttribute("href")).toBe("about:srcdoc")
      expect(
        output.indexOf('http-equiv="Content-Security-Policy"')
      ).toBeLessThan(output.indexOf('<base href="about:srcdoc">'))
    }
  })
})
