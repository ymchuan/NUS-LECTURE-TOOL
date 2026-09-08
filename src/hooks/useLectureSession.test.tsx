// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CourseSettings, StreamingAsrEvent, SummaryPreferences } from "../types";
import { useLectureSession } from "./useLectureSession";

const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

type Listener = (event: { payload: unknown }) => void;
type Session = ReturnType<typeof useLectureSession>;

const settings: CourseSettings = {
  courseId: 7,
  courseCode: "EC2101",
  courseName: "Microeconomics",
  courseContext: "",
  keywords: [],
  hotwords: [{ text: "elasticity", weight: 3 }],
  glossary: [],
  transcriptionModel: "nova-3",
  translationModel: "qwen-mt-turbo",
  summaryModel: "qwen-plus",
};

const preferences: SummaryPreferences = {
  asrConfigVersion: 1,
  textConfigVersion: 1,
  autoSummaryEnabled: false,
  webSearchEnabled: false,
  transcriptionProvider: "deepgram",
  transcriptionModel: "nova-3",
  microphoneDeviceId: "",
  textProvider: "alibaba",
  translationProvider: "alibaba",
  alibabaWorkspaceId: "",
  translationModel: "qwen-mt-turbo",
  localTranslationModel: "translategemma:4b",
  localTranslationEndpoint: "http://127.0.0.1:11434/v1",
  summaryModel: "qwen-plus",
  lectureSummaryModel: "qwen-plus",
};

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((complete) => { resolve = complete; });
  return { promise, resolve };
}

describe("useLectureSession streaming ASR", () => {
  let root: Root;
  let container: HTMLDivElement;
  let session: Session;
  let listeners: Map<string, Set<Listener>>;
  let processors: { onaudioprocess: ((event: unknown) => void) | null }[];
  let track: { enabled: boolean; stop: ReturnType<typeof vi.fn> };

  function Harness({ currentPreferences }: { currentPreferences: SummaryPreferences }) {
    session = useLectureSession(settings, currentPreferences);
    return null;
  }

  function emit(name: string, payload: unknown) {
    listeners.get(name)?.forEach((callback) => callback({ payload }));
  }

  function emitAsr(provider: "deepgram" | "alibaba", event: Partial<StreamingAsrEvent>) {
    emit(`${provider}-asr-event`, {
      kind: "final",
      itemId: "sentence-1",
      text: "Demand is elastic.",
      startMs: 0,
      ...event,
    });
  }

  function sendAudio() {
    const processor = processors[processors.length - 1];
    expect(processor.onaudioprocess).not.toBeNull();
    processor.onaudioprocess?.({
      inputBuffer: { getChannelData: () => new Float32Array([0, 0.5, -0.5]) },
    });
  }

  function defaultInvoke(command: string) {
    if (command === "begin_lecture") return Promise.resolve(42);
    if (command === "list_lecture_document_keywords") return Promise.resolve(["substitution"]);
    if (command === "generate_topic_summary") {
      return Promise.resolve({ id: "lecture-summary", kind: "lecture", startMs: 0, title: "Summary", points: [] });
    }
    return Promise.resolve();
  }

  async function render(currentPreferences = preferences) {
    await act(async () => {
      root.render(<Harness currentPreferences={currentPreferences} />);
    });
  }

  async function start(currentPreferences = preferences) {
    await render(currentPreferences);
    await act(async () => { await session.startLive(); });
    expect(session.status).toBe("live");
  }

  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
    vi.stubGlobal("cancelAnimationFrame", vi.fn());
    listeners = new Map();
    processors = [];
    track = { enabled: true, stop: vi.fn() };
    const stream = { getTracks: () => [track], getAudioTracks: () => [track] };
    vi.stubGlobal("navigator", { mediaDevices: { getUserMedia: vi.fn().mockResolvedValue(stream) } });
    vi.stubGlobal("AudioContext", class {
      sampleRate = 16_000;
      destination = {};
      createAnalyser() {
        return { fftSize: 0, frequencyBinCount: 8, getByteFrequencyData: vi.fn() };
      }
      createMediaStreamSource() { return { connect: vi.fn(), disconnect: vi.fn() }; }
      createScriptProcessor() {
        const processor = { onaudioprocess: null, connect: vi.fn(), disconnect: vi.fn() };
        processors.push(processor);
        return processor;
      }
      createGain() { return { gain: { value: 1 }, connect: vi.fn(), disconnect: vi.fn() }; }
      close() { return Promise.resolve(); }
    });
    tauri.invoke.mockReset().mockImplementation(defaultInvoke);
    tauri.listen.mockReset().mockImplementation((name: string, callback: Listener) => {
      if (!listeners.has(name)) listeners.set(name, new Set());
      listeners.get(name)?.add(callback);
      return Promise.resolve(() => { listeners.get(name)?.delete(callback); });
    });
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => { root.unmount(); });
    container.remove();
    vi.clearAllTimers();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("starts Deepgram with the selected model and sends PCM only to Deepgram", async () => {
    await start();

    expect(tauri.invoke).toHaveBeenCalledWith("start_deepgram_asr", {
      request: {
        model: "nova-3",
        elapsedOffsetMs: 0,
        hotwords: expect.arrayContaining([
          expect.objectContaining({ text: "elasticity" }),
          expect.objectContaining({ text: "substitution" }),
        ]),
        workspaceId: "",
      },
    });
    await act(async () => { sendAudio(); });
    expect(tauri.invoke).toHaveBeenCalledWith("send_deepgram_asr_audio", {
      audioBase64: expect.any(String),
    });
    expect(tauri.invoke.mock.calls.some(([command]) => command === "send_alibaba_asr_audio")).toBe(false);
    expect(tauri.invoke.mock.calls.some(([command]) => command === "create_realtime_call")).toBe(false);
  });

  it("translates a final once, ignores interim corrections after it, and ignores the other provider", async () => {
    await start();
    await act(async () => {
      emitAsr("deepgram", { kind: "delta", text: "Demand is" });
      emitAsr("alibaba", { itemId: "unrelated", text: "Unrelated audio." });
    });
    expect(session.segments).toEqual([expect.objectContaining({ english: "Demand is", state: "interim" })]);
    expect(tauri.invoke.mock.calls.filter(([command]) => command === "translate_segment")).toHaveLength(0);

    await act(async () => {
      emitAsr("deepgram", {});
      emitAsr("deepgram", {});
      emitAsr("deepgram", { kind: "delta", text: "Stale partial" });
    });
    expect(session.segments).toEqual([expect.objectContaining({ english: "Demand is elastic.", state: "translating" })]);
    expect(tauri.invoke.mock.calls.filter(([command]) => command === "translate_segment")).toHaveLength(1);
    expect(tauri.invoke).toHaveBeenCalledWith("translate_segment", {
      request: expect.objectContaining({ segmentId: "sentence-1", english: "Demand is elastic." }),
    });
  });

  it("routes only realtime translation to the configured local model", async () => {
    await start({
      ...preferences,
      translationProvider: "local",
      localTranslationModel: "translategemma:4b",
      localTranslationEndpoint: "http://127.0.0.1:11434/v1",
    });
    await act(async () => { emitAsr("deepgram", {}); });

    expect(tauri.invoke).toHaveBeenCalledWith("translate_segment", {
      request: expect.objectContaining({
        provider: "local",
        model: "translategemma:4b",
        localEndpoint: "http://127.0.0.1:11434/v1",
      }),
    });
  });

  it("drains queued audio and accepts the final tail before saving a paused snapshot", async () => {
    await start();
    const audio = deferred();
    const finish = deferred();
    tauri.invoke.mockClear().mockImplementation((command: string) => {
      if (command === "send_deepgram_asr_audio") return audio.promise;
      if (command === "finish_deepgram_asr") return finish.promise;
      return defaultInvoke(command);
    });
    await act(async () => { sendAudio(); });
    let pause!: Promise<void>;
    await act(async () => { pause = session.togglePause(); });
    expect(track.enabled).toBe(false);
    expect(tauri.invoke).not.toHaveBeenCalledWith("finish_deepgram_asr");
    expect(tauri.invoke.mock.calls.some(([command]) => command === "save_lecture_snapshot")).toBe(false);

    await act(async () => { audio.resolve(); });
    expect(tauri.invoke).toHaveBeenCalledWith("finish_deepgram_asr");
    await act(async () => {
      emitAsr("deepgram", { text: "The last words before pausing." });
      finish.resolve();
      await pause;
    });
    expect(session.status).toBe("paused");
    expect(tauri.invoke).toHaveBeenCalledWith("save_lecture_snapshot", {
      snapshot: expect.objectContaining({
        status: "paused",
        segments: [expect.objectContaining({ english: "The last words before pausing." })],
      }),
    });
    expect(tauri.invoke.mock.calls.filter(([command]) => command === "finish_deepgram_asr")).toHaveLength(1);
    await act(async () => {
      emitAsr("deepgram", { itemId: "late", text: "Arrived after stream was closed." });
    });
    expect(session.segments).toHaveLength(1);
  });

  it("waits for the stop tail and its translation before summarizing and finishing the lecture", async () => {
    await start();
    const finish = deferred();
    const translation = deferred();
    tauri.invoke.mockClear().mockImplementation((command: string) => {
      if (command === "finish_deepgram_asr") return finish.promise;
      if (command === "translate_segment") return translation.promise;
      return defaultInvoke(command);
    });
    let stop!: Promise<void>;
    await act(async () => { stop = session.stop(); });
    expect(tauri.invoke).toHaveBeenCalledWith("finish_deepgram_asr");
    expect(tauri.invoke.mock.calls.some(([command]) => command === "finish_lecture")).toBe(false);

    await act(async () => {
      emitAsr("deepgram", { text: "The lecture ends with this sentence." });
      finish.resolve();
    });
    expect(tauri.invoke.mock.calls.some(([command]) => command === "generate_topic_summary")).toBe(false);
    expect(tauri.invoke.mock.calls.some(([command]) => command === "finish_lecture")).toBe(false);

    await act(async () => {
      emit("translation-event", { segmentId: "sentence-1", kind: "delta", text: "Translated tail." });
      emit("translation-event", { segmentId: "sentence-1", kind: "done", text: "" });
      translation.resolve();
      await stop;
    });
    expect(session.status).toBe("idle");
    expect(track.stop).toHaveBeenCalledOnce();
    expect(tauri.invoke).toHaveBeenCalledWith("generate_topic_summary", {
      request: expect.objectContaining({
        kind: "lecture",
        transcript: expect.stringContaining("The lecture ends with this sentence."),
      }),
    });
    expect(tauri.invoke).toHaveBeenCalledWith("save_lecture_snapshot", {
      snapshot: expect.objectContaining({
        status: "ending",
        segments: [expect.objectContaining({ chinese: "Translated tail.", state: "complete" })],
      }),
    });
    expect(tauri.invoke).toHaveBeenCalledWith("finish_lecture", expect.objectContaining({ lectureId: 42 }));
    const commands = tauri.invoke.mock.calls.map(([command]) => command);
    expect(commands.indexOf("save_lecture_snapshot")).toBeLessThan(commands.indexOf("finish_lecture"));
    expect(commands.filter((command) => command === "finish_deepgram_asr")).toHaveLength(1);
  });

  it("retains the current lecture provider and model across pause and resume after settings change", async () => {
    await start();
    await act(async () => { await session.togglePause(); });
    await render({ ...preferences, transcriptionProvider: "alibaba", transcriptionModel: "qwen3-asr-flash-realtime" });
    tauri.invoke.mockClear();

    await act(async () => { await session.togglePause(); });
    expect(session.status).toBe("live");
    expect(track.enabled).toBe(true);
    expect(tauri.invoke).toHaveBeenCalledWith("start_deepgram_asr", {
      request: expect.objectContaining({ model: "nova-3" }),
    });
    await act(async () => { sendAudio(); });
    expect(tauri.invoke).toHaveBeenCalledWith("send_deepgram_asr_audio", expect.any(Object));
    expect(tauri.invoke.mock.calls.some(([command]) => command === "start_alibaba_asr")).toBe(false);
    expect(tauri.invoke.mock.calls.some(([command]) => command === "begin_lecture")).toBe(false);
  });

  it("preserves the Alibaba start, audio, final-event, and finish routes", async () => {
    await start({ ...preferences, transcriptionProvider: "alibaba", transcriptionModel: "qwen3-asr-flash-realtime" });
    expect(tauri.invoke).toHaveBeenCalledWith("start_alibaba_asr", {
      request: expect.objectContaining({ model: "qwen3-asr-flash-realtime" }),
    });
    await act(async () => {
      sendAudio();
      emitAsr("alibaba", { text: "Alibaba final sentence." });
      emitAsr("deepgram", { itemId: "unrelated", text: "Other provider." });
    });
    expect(tauri.invoke).toHaveBeenCalledWith("send_alibaba_asr_audio", expect.any(Object));
    expect(session.segments).toEqual([expect.objectContaining({ english: "Alibaba final sentence." })]);
    expect(tauri.invoke.mock.calls.filter(([command]) => command === "translate_segment")).toHaveLength(1);
    await act(async () => { await session.togglePause(); });
    expect(tauri.invoke).toHaveBeenCalledWith("finish_alibaba_asr");
    expect(tauri.invoke.mock.calls.some(([command]) => String(command).includes("deepgram"))).toBe(false);
  });
});
