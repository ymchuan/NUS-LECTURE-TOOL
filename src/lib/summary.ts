import type { TopicSummary, TranscriptSegment } from "../types";

const transitionPattern = new RegExp(
  [
    "now (?:let us|let's) move",
    "moving on to",
    "turn(?:ing)? now to",
    "the next (?:topic|concept|part)",
    "to summarize",
    "in summary",
    "that concludes",
    "before we move on",
  ].join("|"),
  "i",
);

function timestamp(milliseconds: number) {
  const seconds = Math.max(0, Math.floor(milliseconds / 1000));
  const minutes = Math.floor(seconds / 60);
  return `${String(minutes).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

function stableSegments(segments: TranscriptSegment[]) {
  return segments.filter(
    (segment) => segment.state !== "interim" && segment.english.trim(),
  );
}

function clipMiddle(value: string, maxLength: number) {
  if (value.length <= maxLength) return value;
  const openingLength = Math.ceil(maxLength * 0.62);
  const closingLength = Math.floor(maxLength * 0.38);
  return `${value.slice(0, openingLength)}\n…\n${value.slice(-closingLength)}`;
}

function evenlySample<T>(items: T[], maxItems: number) {
  if (items.length <= maxItems) return items;
  return Array.from({ length: maxItems }, (_, index) => (
    items[Math.round(index * (items.length - 1) / (maxItems - 1))]
  ));
}

function listSection(label: string, items: string[] | undefined) {
  const values = items?.map((item) => item.trim()).filter(Boolean) ?? [];
  return values.length ? `${label}：\n${values.map((item) => `- ${item}`).join("\n")}` : "";
}

function topicMemoryEntry(summary: TopicSummary, index: number) {
  const knowledgePoints = summary.knowledgePoints?.map((point) => {
    const importance = point.importance === "core" ? "核心" : "补充";
    const evidence = point.lecturerEvidence?.trim()
      ? `；课堂依据：${point.lecturerEvidence.trim()}`
      : "";
    return `${point.title}（${importance}）：${point.explanation}${evidence}`;
  });
  const definitions = summary.definitions?.map(
    (item) => `${item.term}：${item.definition}`,
  );
  const range = summary.endMs === undefined
    ? timestamp(summary.startMs)
    : `${timestamp(summary.startMs)}–${timestamp(summary.endMs)}`;

  return [
    `### 阶段 ${index + 1} [${range}] ${summary.title}`,
    summary.overview?.trim() ? `概览：${summary.overview.trim()}` : "",
    listSection("重点结论", summary.points),
    listSection("知识点与课堂依据", knowledgePoints),
    listSection("定义与术语", definitions),
    listSection("教师例子与类比", summary.examples),
    listSection("考试、作业与易错提示", summary.examTips),
  ].filter(Boolean).join("\n");
}

export function buildLectureTranscriptEvidence(
  segments: TranscriptSegment[],
  maxCharacters = 70_000,
) {
  const buckets = new Map<number, string[]>();
  stableSegments(segments).forEach((segment) => {
    const minute = Math.floor(Math.max(0, segment.startMs) / 60_000);
    const values = buckets.get(minute) ?? [];
    values.push(segment.english.trim());
    buckets.set(minute, values);
  });
  if (!buckets.size) return "";

  const entries = [...buckets.entries()].map(([minute, values]) => (
    `[${timestamp(minute * 60_000)}–${timestamp((minute + 1) * 60_000 - 1)}] ${values.join(" ")}`
  ));
  const complete = entries.join("\n\n");
  if (complete.length <= maxCharacters) return complete;

  const perMinute = Math.max(260, Math.floor(maxCharacters / entries.length) - 2);
  return entries.map((entry) => clipMiddle(entry, perMinute)).join("\n\n");
}

export function buildSummaryTranscript(
  segments: TranscriptSegment[],
  maxCharacters = 24_000,
) {
  const entries = stableSegments(segments)
    .map((segment) => {
      const chinese = segment.chinese.trim() ? `\n中文：${segment.chinese.trim()}` : "";
      return `[${timestamp(segment.startMs)}]\nEnglish: ${segment.english.trim()}${chinese}`;
    });
  const complete = entries.join("\n\n");
  if (complete.length <= maxCharacters) return complete;

  const selectedEntries = evenlySample(
    entries,
    Math.max(2, Math.floor(maxCharacters / 160)),
  );
  const perSegment = Math.max(120, Math.floor(maxCharacters / selectedEntries.length) - 2);
  return selectedEntries
    .map((entry) => clipMiddle(entry, perSegment))
    .join("\n\n");
}

export function buildLectureSummaryInput(
  segments: TranscriptSegment[],
  summaries: TopicSummary[],
) {
  const topicSummaries = summaries
    .filter((summary) => summary.kind !== "lecture")
    .sort((left, right) => left.startMs - right.startMs);
  const memoryEntries = topicSummaries.map(topicMemoryEntry);
  const memoryBudgetPerEntry = memoryEntries.length
    ? Math.max(1_500, Math.floor(80_000 / memoryEntries.length))
    : 0;
  const topicMemory = memoryEntries
    .map((entry) => clipMiddle(entry, memoryBudgetPerEntry))
    .join("\n\n");
  const transcriptEvidence = buildLectureTranscriptEvidence(
    segments,
    topicMemory ? 48_000 : 100_000,
  );

  if (!topicMemory) {
    return `整课原始转写时间轴：\n${transcriptEvidence}`;
  }
  return [
    "阶段总结完整回顾（按课堂顺序）：",
    topicMemory,
    "整课原始转写时间轴（用于补漏和校对）：",
    transcriptEvidence,
  ].join("\n\n");
}

export function shouldAutoSummarize(segments: TranscriptSegment[]) {
  const stable = segments.filter(
    (segment) => segment.state !== "interim" && segment.english.trim(),
  );
  if (stable.length < 5) return false;

  const duration = stable[stable.length - 1].startMs - stable[0].startMs;
  if (duration >= 8 * 60_000) return true;
  if (duration >= 2 * 60_000 && transitionPattern.test(stable[stable.length - 1].english)) {
    return true;
  }
  return duration >= 4 * 60_000 && stable.length >= 10;
}
