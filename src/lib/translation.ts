import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CourseSettings, SummaryPreferences, TranscriptSegment, TranslationEvent } from "../types";

export type ArchivedTranslationTarget = "configured" | "local";

export async function translateArchivedSegment(segment: TranscriptSegment, segments: TranscriptSegment[], settings: CourseSettings, preferences: SummaryPreferences, target: ArchivedTranslationTarget, onUpdate: (segment: TranscriptSegment) => void): Promise<TranscriptSegment> {
  const requestId = crypto.randomUUID();
  const previous = [...segments].reverse().find((item) => item.id !== segment.id && item.state !== "interim" && item.english.trim());
  const provider = target === "local" ? "local" : preferences.translationProvider;
  const model = target === "local" || provider === "local" ? preferences.localTranslationModel : preferences.translationModel;
  let text = "";
  let finish!: () => void;
  let fail!: (error: Error) => void;
  const stream = new Promise<void>((resolve, reject) => { finish = resolve; fail = reject; });
  let active = true;
  const unlisten = await listen<TranslationEvent>("translation-event", ({ payload }) => {
    if (!active || payload.segmentId !== requestId) return;
    if (payload.kind === "delta") { text += payload.text; onUpdate({ ...segment, chinese: text, state: "translating" }); }
    else if (payload.kind === "done") finish();
    else fail(new Error(payload.text));
  });
  try {
    onUpdate({ ...segment, state: "translating" });
    await Promise.race([
      Promise.all([stream, invoke("translate_segment", { request: { segmentId: requestId, english: segment.english, previousEnglish: previous?.english ?? null, courseName: settings.courseName, glossary: settings.glossary, provider, workspaceId: preferences.alibabaWorkspaceId, model, localEndpoint: preferences.localTranslationEndpoint } })]),
      new Promise<never>((_, reject) => window.setTimeout(() => reject(new Error("翻译超时，请稍后重试")), 120_000)),
    ]);
    if (!text.trim()) throw new Error("翻译服务没有返回文本");
    const result = { ...segment, chinese: text, state: "complete" as const };
    onUpdate(result);
    return result;
  } catch (error) {
    onUpdate({ ...segment, state: "error", chinese: segment.chinese || text });
    throw error;
  } finally { active = false; unlisten(); }
}
