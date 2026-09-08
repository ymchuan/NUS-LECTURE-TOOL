import { describe, expect, it } from "vitest";
import {
  isLegacyPowerPointFileName,
  isSupportedSlidesFileName,
  slidesMimeType,
  splitLegacyPowerPointMarkdown,
} from "./powerpoint";

describe("PowerPoint support", () => {
  it("accepts modern and legacy slide files", () => {
    expect(isSupportedSlidesFileName("week-1.pdf")).toBe(true);
    expect(isSupportedSlidesFileName("week-1.pptx")).toBe(true);
    expect(isSupportedSlidesFileName("week-1.PPT")).toBe(true);
    expect(isSupportedSlidesFileName("week-1.doc")).toBe(false);
    expect(isLegacyPowerPointFileName("week-1.PPT")).toBe(true);
  });

  it("assigns the legacy PowerPoint MIME type", () => {
    expect(slidesMimeType("week-1.ppt")).toBe("application/vnd.ms-powerpoint");
    expect(slidesMimeType("week-1.pptx")).toContain("presentationml.presentation");
  });

  it("keeps searchable content grouped by legacy slide titles", () => {
    expect(splitLegacyPowerPointMarkdown(`
## Single-layer perceptron

- Induced local field
- Decision boundary

## Backpropagation

Gradient descent
    `)).toEqual([
      "## Single-layer perceptron\n\n- Induced local field\n- Decision boundary",
      "## Backpropagation\n\nGradient descent",
    ]);
  });

  it("keeps titleless decks usable as one searchable page", () => {
    expect(splitLegacyPowerPointMarkdown("First line\n\nSecond line")).toEqual([
      "First line\n\nSecond line",
    ]);
  });
});
