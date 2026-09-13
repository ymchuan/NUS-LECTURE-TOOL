export function normalizeModelMarkdown(value: string) {
  return value
    .replace(/\r\n?/g, "\n")
    .replace(/\\\[([\s\S]*?)\\\]/g, (_, formula: string) => `$$\n${formula.trim()}\n$$`)
    .replace(/\\\(([\s\S]*?)\\\)/g, (_, formula: string) => `$${formula.trim()}$`)
    .replace(/\\([*#$])/g, "$1")
    .replace(
      /^([ \t]*)\$\$[ \t]*([^\n]*?\S)[ \t]*\$\$[ \t]*$/gm,
      (_, indentation: string, formula: string) => `${indentation}$$\n${indentation}${formula.trim()}\n${indentation}$$`,
    )
    .replace(/\*\*([^*\n]*?\S)\s+\*\*/g, "**$1**")
    .replace(/([\p{L}\p{N}])(\*\*[^*\n]+?\*\*)/gu, "$1<!-- -->$2")
    .replace(/(\*\*[^*\n]+?\*\*)([\p{L}\p{N}])/gu, "$1<!-- -->$2");
}
