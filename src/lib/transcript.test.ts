import { describe, expect, it } from "vitest";
import {
  appendTranslation,
  discardSupersededInterims,
  finalizeTranscript,
  finishTranslation,
  upsertTranscriptDelta,
  upsertTranscriptText,
} from "./transcript";

describe("transcript state", () => {
  it("collects deltas into one interim segment", () => {
    const first = upsertTranscriptDelta([], "item-1", "Elasticity ", 1200);
    const second = upsertTranscriptDelta(first, "item-1", "measures change.", 1300);

    expect(second).toHaveLength(1);
    expect(second[0].english).toBe("Elasticity measures change.");
    expect(second[0].state).toBe("interim");
    expect(second[0].startMs).toBe(1200);
  });

  it("replaces interim text with the final transcript", () => {
    const interim = upsertTranscriptDelta([], "item-1", "A rough draft", 800);
    const final = finalizeTranscript(interim, "item-1", "A corrected sentence.", 900);

    expect(final[0].english).toBe("A corrected sentence.");
    expect(final[0].state).toBe("translating");
  });

  it("preserves an unexpected-script result instead of silently losing lecture content", () => {
    const final = finalizeTranscript([], "item-language-error", "听那个涛声", 900);

    expect(final[0].english).toBe("听那个涛声");
    expect(final[0].state).toBe("translating");
  });

  it("streams and completes a translation", () => {
    const final = finalizeTranscript([], "item-1", "Demand rises.", 1000);
    const partial = appendTranslation(final, "item-1", "需求");
    const translated = appendTranslation(partial, "item-1", "上升。 ");
    const complete = finishTranslation(translated, "item-1");

    expect(complete[0].chinese).toBe("需求上升。 ");
    expect(complete[0].state).toBe("complete");
  });
});

it("replaces cumulative interim text from a sentence-based ASR", () => {
  const first = upsertTranscriptText([], "item-2", "Price", 200);
  const second = upsertTranscriptText(first, "item-2", "Price elasticity", 200);

  expect(second[0].english).toBe("Price elasticity");
});

it("drops abandoned interim items when a new ASR item starts", () => {
  const first = upsertTranscriptText([], "item-1", "Ch", 100);
  const withNext = upsertTranscriptText(first, "item-2", "Channel", 800);

  expect(discardSupersededInterims(withNext, "item-2")).toEqual([
    expect.objectContaining({ itemId: "item-2", english: "Channel" }),
  ]);
});
