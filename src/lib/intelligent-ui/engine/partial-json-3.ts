// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import { PartialJSONResult } from "./partial-json-1"
import { Reader } from "./partial-json-2"

// Parses `text`, tolerating the deviations above and any truncation. Throws
// SyntaxError only for input no continuation could fix (a stray character).
export function parsePartialJSON(text: string): PartialJSONResult {
  return new Reader(text).read()
}
