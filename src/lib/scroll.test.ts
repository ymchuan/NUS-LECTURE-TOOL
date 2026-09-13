import { describe, expect, it } from "vitest";
import { isNearScrollBottom } from "./scroll";

describe("transcript following", () => {
  it("keeps following while the reader is at the bottom", () => {
    expect(isNearScrollBottom({ scrollHeight: 1_000, scrollTop: 552, clientHeight: 400 })).toBe(true);
  });

  it("stops following after the reader scrolls up", () => {
    expect(isNearScrollBottom({ scrollHeight: 1_000, scrollTop: 300, clientHeight: 400 })).toBe(false);
  });

  it("allows a small layout tolerance near the bottom", () => {
    expect(isNearScrollBottom({ scrollHeight: 1_000, scrollTop: 553, clientHeight: 400 })).toBe(true);
    expect(isNearScrollBottom({ scrollHeight: 1_000, scrollTop: 551, clientHeight: 400 })).toBe(false);
  });
});
