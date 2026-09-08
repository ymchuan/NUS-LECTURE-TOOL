import type { AsrHotword, Course, CourseSettings, GlossaryTerm } from "../types";

const DEFAULT_TRANSCRIPTION_MODEL = "gpt-live-transcribe";
const DEFAULT_TRANSLATION_MODEL = "gpt-5.4-nano";
const DEFAULT_SUMMARY_MODEL = "gpt-5.4-mini";

export function mergeAsrHotwords(
  preferred: AsrHotword[],
  supplemental: string[],
): AsrHotword[] {
  const merged = new Map<string, AsrHotword>();
  for (const item of preferred) {
    const text = item.text.trim();
    if (!text) continue;
    const key = text.toLocaleLowerCase();
    const weight = item.weight === 50 ? 50 : Math.max(1, Math.min(5, item.weight));
    const current = merged.get(key);
    if (!current || current.weight < weight) merged.set(key, { text, weight });
  }
  for (const value of supplemental) {
    const text = value.trim();
    const key = text.toLocaleLowerCase();
    if (text && !merged.has(key)) merged.set(key, { text, weight: 2 });
  }
  return [...merged.values()].slice(0, 2_000);
}

export function settingsForCourse(
  course: Course,
  terms: GlossaryTerm[],
): CourseSettings {
  const enabled = terms
    .filter((term) => term.enabled && term.english.trim())
    .sort((left, right) => right.priority - left.priority);

  const hotwordMap = new Map<string, { text: string; weight: number }>();
  for (const item of enabled) {
    const weight = item.priority >= 3 ? 5 : item.priority >= 2 ? 4 : 2;
    const variants = [
      item.english.trim(),
      ...item.aliases
        .split(/[,，;；\n]/)
        .map((alias) => alias.trim())
        .filter(Boolean),
    ];
    for (const text of variants) {
      const key = text.toLocaleLowerCase();
      const current = hotwordMap.get(key);
      if (!current || current.weight < weight) hotwordMap.set(key, { text, weight });
    }
  }
  const hotwords = [...hotwordMap.values()].slice(0, 2_000);

  const glossary = enabled.map((term) => {
    const translation = term.chinese.trim() || "保留英文";
    const aliases = term.aliases.trim() ? `；别名：${term.aliases.trim()}` : "";
    return `${term.english.trim()} → ${translation}${aliases}`;
  });

  return {
    courseId: course.id,
    courseCode: course.code,
    courseName: course.name,
    courseContext: course.description,
    keywords: hotwords.map((item) => item.text).slice(0, 200),
    hotwords,
    glossary,
    transcriptionModel: DEFAULT_TRANSCRIPTION_MODEL,
    translationModel: DEFAULT_TRANSLATION_MODEL,
    summaryModel: DEFAULT_SUMMARY_MODEL,
  };
}

export function emptyCourseSettings(): CourseSettings {
  return {
    courseId: null,
    courseCode: "",
    courseName: "NUS Lecture",
    courseContext: "An undergraduate university lecture in Singapore.",
    keywords: [],
    hotwords: [],
    glossary: [],
    transcriptionModel: DEFAULT_TRANSCRIPTION_MODEL,
    translationModel: DEFAULT_TRANSLATION_MODEL,
    summaryModel: DEFAULT_SUMMARY_MODEL,
  };
}
