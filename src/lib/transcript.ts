import type { TranscriptSegment } from "../types";

export function upsertTranscriptDelta(
  segments: TranscriptSegment[],
  itemId: string,
  delta: string,
  startMs: number,
): TranscriptSegment[] {
  const index = segments.findIndex((segment) => segment.itemId === itemId);
  if (index === -1) {
    return [
      ...segments,
      {
        id: itemId,
        itemId,
        startMs,
        english: delta,
        chinese: "",
        state: "interim",
      },
    ];
  }

  return segments.map((segment, currentIndex) =>
    currentIndex === index
      ? { ...segment, english: `${segment.english}${delta}` }
      : segment,
  );
}

export function finalizeTranscript(
  segments: TranscriptSegment[],
  itemId: string,
  transcript: string,
  startMs: number,
): TranscriptSegment[] {
  const existing = segments.find((segment) => segment.itemId === itemId);
  if (!existing) {
    return [
      ...segments,
      {
        id: itemId,
        itemId,
        startMs,
        english: transcript.trim(),
        chinese: "",
        state: "translating",
      },
    ];
  }

  return segments.map((segment) =>
    segment.itemId === itemId
      ? { ...segment, english: transcript.trim(), state: "translating" }
      : segment,
  );
}

export function upsertTranscriptText(
  segments: TranscriptSegment[],
  itemId: string,
  text: string,
  startMs: number,
): TranscriptSegment[] {
  const existing = segments.some((segment) => segment.itemId === itemId);
  if (!existing) {
    return [
      ...segments,
      {
        id: itemId,
        itemId,
        startMs,
        english: text,
        chinese: "",
        state: "interim",
      },
    ];
  }

  return segments.map((segment) =>
    segment.itemId === itemId ? { ...segment, english: text } : segment,
  );
}

export function discardSupersededInterims(
  segments: TranscriptSegment[],
  activeItemId: string,
): TranscriptSegment[] {
  return segments.filter(
    (segment) => segment.state !== "interim" || segment.itemId === activeItemId,
  );
}

export function appendTranslation(
  segments: TranscriptSegment[],
  segmentId: string,
  delta: string,
): TranscriptSegment[] {
  return segments.map((segment) =>
    segment.id === segmentId
      ? { ...segment, chinese: `${segment.chinese}${delta}` }
      : segment,
  );
}

export function finishTranslation(
  segments: TranscriptSegment[],
  segmentId: string,
  hasError = false,
): TranscriptSegment[] {
  return segments.map((segment) =>
    segment.id === segmentId
      ? { ...segment, state: hasError ? "error" : "complete" }
      : segment,
  );
}
