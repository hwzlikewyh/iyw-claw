// ExcelJS的解析器按无前缀名称匹配主命名空间；保留其它命名空间及r:id属性。
export function patchSpreadsheet(source) {
  const changes = [
    [
      "const saxesParser = new SaxesParser();",
      "const saxesParser = new SaxesParser({ xmlns: true });",
    ],
    [
      'saxesParser.on("opentag", (value) => events.push({ eventType: "opentag", value }));',
      'saxesParser.on("opentag", (value) => events.push({ eventType: "opentag", value: normalizeOfficeTag(value) }));',
    ],
    [
      'saxesParser.on("closetag", (value) => events.push({ eventType: "closetag", value }));',
      'saxesParser.on("closetag", (value) => events.push({ eventType: "closetag", value: normalizeOfficeTag(value) }));',
    ],
    [
      'let events = [];\n      saxesParser.on("opentag"',
      'function normalizeOfficeTag(value) { return { ...value, name: ["http://schemas.openxmlformats.org/spreadsheetml/2006/main", "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"].includes(value.uri) ? value.local : value.name, attributes: Object.fromEntries(Object.values(value.attributes).map(attribute => [attribute.name, attribute.value])) }; }\n      let events = [];\n      saxesParser.on("opentag"',
    ],
  ]
  for (const [before, after] of changes) {
    if (source.split(before).length !== 2)
      throw Error("Upstream spreadsheet XML parser changed")
    source = source.replace(before, after)
  }
  return source
}
