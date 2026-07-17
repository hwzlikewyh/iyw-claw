import { describe, expect, it } from "vitest"

import { textToDoc } from "@/components/chat/composer/plain-text-content"

describe("textToDoc", () => {
  it("preserves line breaks as hard-break nodes", () => {
    expect(textToDoc("one\n\nthree")).toEqual({
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [
            { type: "text", text: "one" },
            { type: "hardBreak" },
            { type: "hardBreak" },
            { type: "text", text: "three" },
          ],
        },
      ],
    })
  })
})
