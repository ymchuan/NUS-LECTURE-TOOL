import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { RichMessage } from "./RichMessage";

describe("RichMessage", () => {
  it("renders intraword Chinese emphasis as strong text", () => {
    const html = renderToStaticMarkup(<RichMessage content="二项分布可以用**泊松分布**来近似" />);

    expect(html).toContain("<strong>泊松分布</strong>");
    expect(html).not.toContain("**");
  });

  it("renders display LaTeX as a mathematical formula", () => {
    const html = renderToStaticMarkup(
      <RichMessage content={String.raw`定义与公式：
1. **定义与公式：**
    二项分布的概率为：
    $$ P(K=k) = \binom{N}{k} p^k (1-p)^{N-k} $$
    其中 $\binom{N}{k}$ 是组合数。
2. **应用场景：**
    二项分布可以用**泊松分布**来近似。`} />,
    );

    expect(html).toContain("katex-display");
    expect(html).toContain("mfrac");
    expect(html).toContain("<strong>泊松分布</strong>");
    expect(html).toContain("<strong>应用场景：</strong>");
    expect(html).not.toContain("$\\binom");
    expect(html).not.toContain("$$");
    expect(html).not.toContain("<!-- -->");
  });
});
