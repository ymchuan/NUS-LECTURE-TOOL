import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CourseSettings, SummaryPreferences, TranscriptSegment, TranslationEvent, TranslationTarget } from "../types";

export function precedingEnglish(segments: TranscriptSegment[], segment: TranscriptSegment) {
  const index = segments.findIndex((item) => item.id === segment.id);
  const earlier = index >= 0 ? segments.slice(0, index) : segments;
  return [...earlier].reverse().find((item) =>
    item.state !== "interim" && item.english.trim() && item.startMs <= segment.startMs,
  )?.english ?? null;
}

export async function translateTranscriptSegment({
  segment, segments, settings, preferences, target = "configured", onUpdate,
}: {
  segment: TranscriptSegment;
  segments: TranscriptSegment[];
  settings: CourseSettings;
  preferences: SummaryPreferences;
  target?: TranslationTarget;
  onUpdate: (segment: TranscriptSegment) => void;
}): Promise<TranscriptSegment> {
  const provider = target === "ollama-local" ? "ollama"
    : target === "ollama-cloud" ? "ollama-cloud" : preferences.textProvider;
  const model = target === "ollama-local" ? preferences.ollamaTranslationModel
    : target === "ollama-cloud" ? preferences.ollamaCloudTranslationModel : preferences.translationModel;
  // A distinct wire ID prevents a late response from an older attempt corrupting a retry.
  const requestId = crypto.randomUUID();
  let text = "";
  let active = true;
  let timeout: ReturnType<typeof setTimeout> | undefined;
  let unlisten: (() => void) | undefined;
  const update = (next: TranscriptSegment) => { if (active) onUpdate(next); };
  let finishStream!: () => void;
  let failStream!: (reason: Error) => void;
  const streamFinished = new Promise<void>((resolve, reject) => {
    finishStream = resolve;
    failStream = reject;
  });
  update({ ...segment, state: "translating" });
  try {
    if (target === "ollama-cloud" && !model.trim()) {
      throw new Error("请先在补翻设置中填写 Ollama 云端模型名称");
    }
    unlisten = await listen<TranslationEvent>("translation-event", ({ payload }) => {
      if (!active || payload.segmentId !== requestId) return;
      if (payload.kind === "delta") {
        text += payload.text;
        update({ ...segment, chinese: text, state: "translating" });
      } else if (payload.kind === "done") {
        finishStream();
      } else {
        failStream(new Error(payload.text));
      }
    });
    await Promise.race([
      Promise.all([streamFinished, invoke("translate_segment", {
        request: {
          segmentId: requestId,
          english: segment.english,
          previousEnglish: precedingEnglish(segments, segment),
          courseName: settings.courseName,
          glossary: settings.glossary,
          provider,
          workspaceId: preferences.alibabaWorkspaceId,
          openaiBaseUrl: preferences.openaiBaseUrl,
          ollamaBaseUrl: preferences.ollamaBaseUrl,
          openaiKeySlot: "translation",
          model,
        },
      })]),
      new Promise<never>((_, reject) => {
        timeout = setTimeout(() => reject(new Error("翻译超时，英文已保留，可以重新翻译")),
          provider === "ollama" ? 120_000 : 65_000);
      }),
    ]);
    if (!text.trim()) throw new Error("翻译未完整返回，可以重新翻译");
    const result: TranscriptSegment = { ...segment, chinese: text, state: "complete" };
    update(result);
    return result;
  } catch (reason) {
    // Keep a prior translation on failure; a first attempt may retain its partial output.
    update({ ...segment, chinese: segment.chinese || text, state: "error" });
    throw reason;
  } finally {
    active = false;
    if (timeout !== undefined) clearTimeout(timeout);
    unlisten?.();
  }
}
