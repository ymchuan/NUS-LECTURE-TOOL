import { describe, expect, it } from "vitest";
import { normalizeModelMarkdown } from "./markdown";

describe("normalizeModelMarkdown", () => {
  it("repairs escaped emphasis and math delimiters without changing LaTeX commands", () => {
    const source = String.raw`\*\*Binom\*\* uses \$N\$ trials.

$$ P(K=k) = \binom{N}{k} p^k (1-p)^{N-k} $$`;
    const normalized = normalizeModelMarkdown(source);

    expect(normalized).toContain("**Binom**");
    expect(normalized).toContain("$N$");
    expect(normalized).toContain(String.raw`\binom{N}{k}`);
  });

  it("converts bracket-style display and inline formulas", () => {
    const source = String.raw`\[E = mc^2\] and \(p^k\)`;
    expect(normalizeModelMarkdown(source)).toBe("$$\nE = mc^2\n$$ and $p^k$");
  });

  it("converts a same-line double-dollar formula into a display block", () => {
    expect(normalizeModelMarkdown("$$ P(K=k) = p^k $$")).toBe("$$\nP(K=k) = p^k\n$$");
  });

  it("preserves list indentation on every line of a display formula", () => {
    expect(normalizeModelMarkdown("    $$ P(K=k) = p^k $$")).toBe(
      "    $$\n    P(K=k) = p^k\n    $$",
    );
  });

  it("removes whitespace that prevents a bold closing delimiter", () => {
    expect(normalizeModelMarkdown("**核心要点： ** 内容")).toBe("**核心要点：** 内容");
  });

  it("separates strong markers from adjacent Chinese text without visible spaces", () => {
    expect(normalizeModelMarkdown("可以用**泊松分布**来近似")).toBe(
      "可以用<!-- -->**泊松分布**<!-- -->来近似",
    );
  });
});
