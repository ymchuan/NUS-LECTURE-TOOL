import { describe, expect, it } from "vitest";
import { requiredSlideConfirmations } from "./slides";

describe("slide match stabilization", () => {
  it("accepts an initial or next-page match after two independent segments", () => {
    expect(requiredSlideConfirmations(null, null, 1, 3)).toBe(2);
    expect(requiredSlideConfirmations(1, 7, 1, 8)).toBe(2);
  });

  it("requires extra evidence for jumps, backwards moves, and document changes", () => {
    expect(requiredSlideConfirmations(1, 7, 1, 10)).toBe(3);
    expect(requiredSlideConfirmations(1, 7, 1, 6)).toBe(3);
    expect(requiredSlideConfirmations(1, 7, 2, 8)).toBe(3);
  });
});
