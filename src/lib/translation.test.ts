import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { emptyCourseSettings } from "./courses";
import { precedingEnglish, translateTranscriptSegment } from "./translation";
import type { SummaryPreferences, TranscriptSegment, TranslationEvent, TranslationTarget } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const preferences: SummaryPreferences = {
  asrConfigVersion: 4, textConfigVersion: 5, autoSummaryEnabled: true, webSearchEnabled: true,
  transcriptionProvider: "deepgram", transcriptionModel: "nova-3", microphoneDeviceId: "",
  textProvider: "openai", alibabaWorkspaceId: "", openaiBaseUrl: "https://example.com/v1",
  ollamaBaseUrl: "http://127.0.0.1:11434", ollamaTranslationModel: "qwen2.5:3b",
  ollamaCloudTranslationModel: "emergency-cloud-model",
  translationModel: "cloud-translation", summaryModel: "cloud-summary", lectureSummaryModel: "cloud-lecture",
};
const segment = (id: string, startMs = 0): TranscriptSegment => ({
  id, itemId: id, startMs, english: `Sentence ${id}`, chinese: "原译文", state: "complete",
});
let listener: (event: Event<TranslationEvent>) => void;
let unlisten = vi.fn<() => void>();
const emit = (id: string, kind: TranslationEvent["kind"], text = "") =>
  listener({ event: "translation-event", id: 0, payload: { segmentId: id, kind, text } });
const lastRequest = () => {
  const calls = vi.mocked(invoke).mock.calls;
  return (calls[calls.length - 1][1] as { request: { segmentId: string } }).request;
};
const requestId = () => lastRequest().segmentId;
const start = (onUpdate = vi.fn(), target: TranslationTarget = "configured") => translateTranscriptSegment({
  segment: segment("b", 1000), segments: [segment("a"), segment("b", 1000), segment("c", 2000)],
  settings: emptyCourseSettings(), preferences, onUpdate, target,
});

beforeEach(() => {
  vi.clearAllMocks();
  unlisten = vi.fn();
  vi.mocked(listen).mockImplementation(async (_, callback) => {
    listener = callback as typeof listener;
    return unlisten;
  });
});
afterEach(() => vi.useRealTimers());

describe("segment retranslation", () => {
  it("uses the preceding sentence, not the latest sentence, when retrying old text", () => {
    const segments = [segment("a"), segment("b", 1000), segment("c", 2000)];
    expect(precedingEnglish(segments, segments[1])).toBe("Sentence a");
    expect(precedingEnglish(segments, segments[0])).toBeNull();
  });

  it("waits for a terminal event even if the IPC reply arrives first", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const onUpdate = vi.fn();
    const pending = start(onUpdate);
    await Promise.resolve();
    const id = requestId();
    expect(id).not.toBe("b");
    expect(onUpdate.mock.lastCall?.[0].chinese).toBe("原译文");
    emit("another-request", "delta", "不属于这句");
    emit(id, "delta", "新译文");
    expect(onUpdate.mock.lastCall?.[0].state).toBe("translating");
    emit(id, "done");
    expect(await pending).toMatchObject({ id: "b", chinese: "新译文", state: "complete" });
    expect(unlisten).toHaveBeenCalledOnce();
    emit(id, "delta", "迟到");
    expect(onUpdate.mock.lastCall?.[0].chinese).toBe("新译文");
  });

  it("restores an existing translation if the replacement stream fails", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const onUpdate = vi.fn();
    const pending = start(onUpdate);
    const rejected = expect(pending).rejects.toThrow("network interrupted");
    await Promise.resolve();
    emit(requestId(), "delta", "不完整的新译文");
    emit(requestId(), "error", "network interrupted");
    await rejected;
    expect(onUpdate.mock.lastCall?.[0]).toMatchObject({ english: "Sentence b", chinese: "原译文", state: "error" });
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("marks a timed-out request retryable and ignores its late events", async () => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockImplementation(() => new Promise(() => {}));
    const onUpdate = vi.fn();
    const pending = start(onUpdate);
    const rejected = expect(pending).rejects.toThrow("翻译超时");
    await Promise.resolve();
    const id = requestId();
    await vi.advanceTimersByTimeAsync(65_000);
    await rejected;
    emit(id, "delta", "过期译文");
    expect(onUpdate.mock.lastCall?.[0]).toMatchObject({ chinese: "原译文", state: "error" });
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("uses the local override without changing the configured cloud preferences", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const pending = start(vi.fn(), "ollama-local");
    await Promise.resolve();
    expect(lastRequest()).toMatchObject({
      provider: "ollama", model: "qwen2.5:3b", ollamaBaseUrl: "http://127.0.0.1:11434",
      previousEnglish: "Sentence a",
    });
    emit(requestId(), "delta", "本地译文");
    emit(requestId(), "done");
    await pending;
    expect(preferences.textProvider).toBe("openai");
    expect(preferences.translationModel).toBe("cloud-translation");
    expect(preferences.summaryModel).toBe("cloud-summary");
    expect(preferences.lectureSummaryModel).toBe("cloud-lecture");
  });

  it("uses the Ollama cloud model only for an explicit emergency retry", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const pending = start(vi.fn(), "ollama-cloud");
    await Promise.resolve();
    expect(lastRequest()).toMatchObject({
      provider: "ollama-cloud", model: "emergency-cloud-model", ollamaBaseUrl: "http://127.0.0.1:11434",
      openaiKeySlot: "translation", previousEnglish: "Sentence a",
    });
    emit(requestId(), "delta", "云端补翻");
    emit(requestId(), "done");
    await pending;
    expect(preferences.textProvider).toBe("openai");
    expect(preferences.translationModel).toBe("cloud-translation");
    expect(preferences.summaryModel).toBe("cloud-summary");
    expect(preferences.lectureSummaryModel).toBe("cloud-lecture");
  });

  it("keeps automatic translation on the configured provider after an Ollama retry", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const local = start(vi.fn(), "ollama-local");
    await Promise.resolve();
    emit(requestId(), "delta", "本地补翻");
    emit(requestId(), "done");
    await local;

    const automatic = start();
    await Promise.resolve();
    expect(lastRequest()).toMatchObject({
      provider: "openai", model: "cloud-translation", openaiKeySlot: "translation",
    });
    emit(requestId(), "delta", "实时翻译");
    emit(requestId(), "done");
    await automatic;
  });

  it.each([
    ["ollama-local", 120_000],
    ["ollama-cloud", 65_000],
  ] as const)("uses the correct timeout for %s", async (target, timeoutMs) => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockImplementation(() => new Promise(() => {}));
    const onUpdate = vi.fn();
    const pending = start(onUpdate, target);
    const rejected = expect(pending).rejects.toThrow("翻译超时");
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(timeoutMs - 1);
    expect(onUpdate.mock.lastCall?.[0].state).toBe("translating");
    await vi.advanceTimersByTimeAsync(1);
    await rejected;
    expect(onUpdate.mock.lastCall?.[0]).toMatchObject({ chinese: "原译文", state: "error" });
  });

  it("requires a configured Ollama cloud model", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const pending = translateTranscriptSegment({
      segment: segment("cloud"), segments: [], settings: emptyCourseSettings(),
      preferences: { ...preferences, ollamaCloudTranslationModel: "" },
      target: "ollama-cloud", onUpdate: vi.fn(),
    });
    await expect(pending).rejects.toThrow("Ollama 云端模型");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("keeps English and partial output on a first-attempt failure", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("transport failed"));
    const onUpdate = vi.fn();
    await expect(translateTranscriptSegment({
      segment: { ...segment("x"), chinese: "" }, segments: [],
      settings: emptyCourseSettings(), preferences, onUpdate,
    })).rejects.toThrow("transport failed");
    expect(onUpdate.mock.lastCall?.[0]).toMatchObject({ english: "Sentence x", state: "error" });
  });
});
