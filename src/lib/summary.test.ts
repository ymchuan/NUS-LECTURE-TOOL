import { describe, expect, it } from "vitest";
import type { TranscriptSegment } from "../types";
import {
  buildLectureSummaryInput,
  buildLectureTranscriptEvidence,
  buildSummaryTranscript,
  shouldAutoSummarize,
} from "./summary";

function segment(index: number, startMs: number, english = `Sentence ${index}`): TranscriptSegment {
  return {
    id: `segment-${index}`,
    itemId: `segment-${index}`,
    startMs,
    english,
    chinese: `语句 ${index}`,
    state: "complete",
  };
}

describe("summary trigger", () => {
  it.each([
    [4, 480_000, false], [5, 479_999, false], [5, 480_000, true],
    [9, 240_000, false], [10, 239_999, false], [10, 240_000, true],
  ])("requires the count and time boundaries (%i segments, %i ms)", (count, duration, expected) => {
    const items = Array.from({ length: count }, (_, index) => segment(index, index * duration / (count - 1)));
    expect(shouldAutoSummarize(items)).toBe(expected);
  });

  it("can summarize English despite a failed translation, but excludes interim text", () => {
    const items = Array.from({ length: 5 }, (_, index) => ({ ...segment(index, index * 120_000), state: "error" as const }));
    expect(shouldAutoSummarize(items)).toBe(true);
    expect(shouldAutoSummarize([...items.slice(0, 4), { ...items[4], state: "interim" }])).toBe(false);
  });

  it("recognizes a topic transition after the minimum duration", () => {
    const segments = [
      segment(1, 0),
      segment(2, 30_000),
      segment(3, 60_000),
      segment(4, 90_000),
      segment(5, 125_000, "Now let's move on to income elasticity."),
    ];
    expect(shouldAutoSummarize(segments)).toBe(true);
  });

  it("does not summarize a short fragment", () => {
    expect(shouldAutoSummarize([segment(1, 0), segment(2, 10_000)])).toBe(false);
  });

  it("builds bilingual evidence with timestamps", () => {
    const transcript = buildSummaryTranscript([segment(1, 65_000, "Demand changes.")]);
    expect(transcript).toContain("[01:05]");
    expect(transcript).toContain("English: Demand changes.");
    expect(transcript).toContain("中文：语句 1");
  });

  it("keeps the beginning of a stage with more than forty segments", () => {
    const segments = Array.from({ length: 70 }, (_, index) => (
      segment(index, index * 5_000, `Stage sentence ${index}`)
    ));
    const transcript = buildSummaryTranscript(segments);

    expect(transcript).toContain("Stage sentence 0");
    expect(transcript).toContain("Stage sentence 69");
  });

  it("samples an unusually long pending stage across its full timeline", () => {
    const segments = Array.from({ length: 1_000 }, (_, index) => (
      segment(index, index * 5_000, `Long stage sentence ${index} with supporting detail`)
    ));
    const transcript = buildSummaryTranscript(segments, 2_000);

    expect(transcript).toContain("Long stage sentence 0");
    expect(transcript).toContain("Long stage sentence 999");
    expect(transcript.length).toBeLessThan(2_200);
  });

  it("builds lecture memory from topic summaries and recent evidence", () => {
    const input = buildLectureSummaryInput(
      [segment(1, 0, "English evidence")],
      [{
        id: "summary-1",
        kind: "topic",
        startMs: 0,
        title: "Demand",
        points: ["Prices affect quantity demanded"],
        overview: "Demand curves summarize willingness to pay.",
      }],
    );

    expect(input).toContain("阶段总结完整回顾");
    expect(input).toContain("Demand curves summarize willingness to pay.");
    expect(input).toContain("English evidence");
  });

  it("keeps detailed topic knowledge in the whole-lecture input", () => {
    const input = buildLectureSummaryInput(
      [segment(1, 0, "The lecturer derives the Erlang traffic equation.")],
      [{
        id: "summary-rich",
        kind: "topic",
        startMs: 60_000,
        endMs: 240_000,
        title: "Traffic intensity",
        overview: "Traffic intensity links call frequency and holding time.",
        points: ["Traffic intensity is measured in Erlangs."],
        knowledgePoints: [{
          title: "Erlang equation",
          explanation: "Multiply calls per hour by average holding time in hours.",
          lecturerEvidence: "The lecturer calculated one call lasting 120 seconds.",
          importance: "core",
        }],
        definitions: [{ term: "Erlang", definition: "Average channel occupancy." }],
        examples: ["One 120-second call per hour equals 1/30 Erlang."],
        examTips: ["Keep the time units consistent."],
      }],
    );

    expect(input).toContain("知识点与课堂依据");
    expect(input).toContain("Multiply calls per hour");
    expect(input).toContain("Average channel occupancy");
    expect(input).toContain("1/30 Erlang");
    expect(input).toContain("Keep the time units consistent");
  });

  it("covers the beginning and end of a long lecture transcript", () => {
    const segments = Array.from({ length: 121 }, (_, index) => (
      segment(index, index * 60_000, `Minute ${index} evidence`)
    ));
    const evidence = buildLectureTranscriptEvidence(segments, 20_000);

    expect(evidence).toContain("Minute 0 evidence");
    expect(evidence).toContain("Minute 60 evidence");
    expect(evidence).toContain("Minute 120 evidence");
  });

  it("keeps every stage while bounding a large whole-lecture request", () => {
    const segments = Array.from({ length: 181 }, (_, index) => (
      segment(index, index * 60_000, `Minute ${index} lecture evidence ${"x".repeat(500)}`)
    ));
    const summaries = Array.from({ length: 40 }, (_, index) => ({
      id: `summary-${index}`,
      kind: "topic" as const,
      startMs: index * 4 * 60_000,
      endMs: (index + 1) * 4 * 60_000,
      title: `Topic ${index}`,
      overview: `Overview ${index} ${"y".repeat(3_000)}`,
      points: [`Point ${index}`],
      knowledgePoints: [],
    }));
    const input = buildLectureSummaryInput(segments, summaries);

    expect(input).toContain("Topic 0");
    expect(input).toContain("Topic 39");
    expect(input).toContain("Minute 0 lecture evidence");
    expect(input).toContain("Minute 180 lecture evidence");
    expect(input.length).toBeLessThan(130_000);
  });
});
