import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  appendTranslation,
  discardSupersededInterims,
  finalizeTranscript,
  finishTranslation,
  replaceTranslation,
  upsertTranscriptDelta,
  upsertTranscriptText,
} from "../lib/transcript";
import { downsampleToPcm16, pcmToBase64 } from "../lib/audio";
import {
  buildLectureSummaryInput,
  buildSummaryTranscript,
  shouldAutoSummarize,
} from "../lib/summary";
import { summaryModelForKind } from "../lib/modelPreferences";
import { mergeAsrHotwords } from "../lib/courses";
import type {
  CourseSettings,
  StreamingAsrEvent,
  LectureSnapshot,
  SessionStatus,
  SummaryPreferences,
  TopicSummary,
  TranscriptSegment,
  TranslationEvent,
  TranscriptionProvider,
} from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;
type StreamingProvider = Exclude<TranscriptionProvider, "openai">;

export interface StartLectureOptions {
  title?: string;
  slides?: {
    name: string;
    mimeType: string;
    dataBase64: string;
    parsedPages?: string[];
  };
}

const demoSegments = [
  {
    english:
      "Today we are looking at price elasticity of demand and why responsiveness matters.",
    chinese: "今天我们讨论需求价格弹性，以及为什么反应程度很重要。",
  },
  {
    english:
      "If a small change in price leads to a large change in quantity demanded, demand is elastic.",
    chinese: "如果价格的小幅变化导致需求量大幅变化，那么需求就是富有弹性的。",
  },
  {
    english:
      "The coefficient is the percentage change in quantity divided by the percentage change in price.",
    chinese: "弹性系数等于需求量的百分比变化除以价格的百分比变化。",
  },
  {
    english:
      "Notice that necessities usually have fewer substitutes, so their demand tends to be inelastic.",
    chinese: "必需品通常缺少替代品，因此其需求往往缺乏弹性。",
  },
  {
    english:
      "We will now move on to income elasticity and distinguish normal goods from inferior goods.",
    chinese: "接下来讨论收入弹性，并区分正常品与劣等品。",
  },
];

function formatDemoSummary(startMs: number, kind: "topic" | "lecture" = "topic"): TopicSummary {
  const isLecture = kind === "lecture";
  return {
    id: `summary-${Date.now()}`,
    kind,
    startMs: kind === "lecture" ? 0 : startMs,
    title: kind === "lecture" ? "整课总结：需求弹性" : "需求价格弹性",
    overview: isLecture
      ? "本次演示课围绕需求价格弹性的含义、计算方式和影响因素展开。教师先说明弹性用于衡量需求量对价格变化的反应程度，再给出百分比变化之比的计算关系，随后用替代品较少的必需品解释需求为何可能缺乏弹性。课程最后转入收入弹性，为后续区分正常品与劣等品建立衔接。"
      : "需求价格弹性描述价格变化时需求量的反应程度。课堂通过弹性系数、替代品和必需品解释了影响弹性的关键因素。",
    points: [
      "弹性衡量需求量对价格变化的敏感程度",
      "需求量变化百分比除以价格变化百分比",
      "替代品较少的必需品通常缺乏弹性",
      ...(isLecture ? ["本节末尾由价格弹性转向收入弹性，并将继续区分正常品与劣等品"] : []),
    ],
    knowledgePoints: [
      {
        title: "弹性系数",
        explanation: "需求量变化百分比除以价格变化百分比，用于比较不同商品的价格敏感度。",
        lecturerEvidence: "教师给出了百分比变化的计算关系。",
        importance: "core",
      },
      {
        title: "替代品与必需品",
        explanation: "替代品越少，消费者越难因价格变化调整需求，因此通常更缺乏弹性。",
        lecturerEvidence: "教师以必需品作为缺乏弹性的例子。",
        importance: "supporting",
      },
      ...(isLecture ? [{
        title: "弹性的计算逻辑",
        explanation: "计算时比较需求量和价格各自的百分比变化，而不是直接比较绝对变化量。这使不同价格和销量尺度的商品可以使用同一指标讨论反应程度。",
        lecturerEvidence: "教师明确给出需求量百分比变化除以价格百分比变化的关系。",
        importance: "core" as const,
      }, {
        title: "与后续主题的衔接",
        explanation: "在完成价格弹性的基本讨论后，课程转向收入弹性，并准备使用收入变化来区分正常品与劣等品。",
        lecturerEvidence: "演示最后一句明确宣布转入 income elasticity。",
        importance: "supporting" as const,
      }] : []),
    ],
    definitions: [
      { term: "Price elasticity of demand", definition: "需求量对价格变化的敏感程度。" },
    ],
    examples: ["必需品通常替代品较少，因此需求往往缺乏弹性。"],
    examTips: [],
    questions: [
      "需求价格弹性的计算公式是什么？",
      "为什么替代品数量会影响需求弹性？",
      ...(isLecture ? ["为什么计算弹性时使用百分比变化？", "本节课如何衔接到收入弹性？"] : []),
    ],
    mindMap: {
      root: "需求价格弹性",
      branches: [
        {
          label: "衡量方法",
          children: [
            { label: "需求量变化百分比", note: "作为分子" },
            { label: "价格变化百分比", note: "作为分母" },
          ],
        },
        {
          label: "影响因素",
          children: [
            { label: "替代品", note: "替代品越多，通常越有弹性" },
            { label: "必需程度", note: "必需品通常更缺乏弹性" },
          ],
        },
        ...(isLecture ? [{
          label: "课程衔接",
          children: [
            { label: "价格弹性", note: "本节已经完成的核心主题" },
            { label: "收入弹性", note: "下一步用于区分不同商品类型" },
          ],
        }] : []),
      ],
    },
    webInsights: [],
    sources: [],
    webEnriched: false,
    model: "demo",
    isDemo: true,
  };
}

export function useLectureSession(
  settings: CourseSettings,
  summaryPreferences: SummaryPreferences,
) {
  const [status, setStatus] = useState<SessionStatus>("idle");
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [summaries, setSummaries] = useState<TopicSummary[]>([]);
  const [elapsedMs, setElapsedMs] = useState(0);
  const [audioLevel, setAudioLevel] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [summaryStatus, setSummaryStatus] = useState<"idle" | "generating" | "searching">("idle");
  const [isEnding, setIsEnding] = useState(false);
  const [asrLatencyMs, setAsrLatencyMs] = useState<number | null>(null);
  const [translationLatencyMs, setTranslationLatencyMs] = useState<number | null>(null);
  const [translationTotalMs, setTranslationTotalMs] = useState<number | null>(null);
  const [lectureId, setLectureId] = useState<number | null>(null);

  const segmentsRef = useRef<TranscriptSegment[]>([]);
  const summariesRef = useRef<TopicSummary[]>([]);
  const elapsedMsRef = useRef(0);
  const statusRef = useRef<SessionStatus>("idle");
  const lectureIdRef = useRef<number | null>(null);
  const summaryCursorRef = useRef(0);
  const summaryInFlightRef = useRef(false);
  const endingRef = useRef(false);
  const peerRef = useRef<RTCPeerConnection | null>(null);
  const channelRef = useRef<RTCDataChannel | null>(null);
  const openAiFinalizationRef = useRef<(() => void) | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const audioSourceRef = useRef<MediaStreamAudioSourceNode | null>(null);
  const audioProcessorRef = useRef<ScriptProcessorNode | null>(null);
  const silentGainRef = useRef<GainNode | null>(null);
  const asrAudioQueueRef = useRef<Promise<void>>(Promise.resolve());
  const streamingAsrActiveRef = useRef<StreamingProvider | null>(null);
  const streamingAsrReceivingRef = useRef<StreamingProvider | null>(null);
  const streamingAsrFinishRef = useRef<Promise<void>>(Promise.resolve());
  const streamingAsrFinishingRef = useRef(false);
  const pauseTransitionRef = useRef<Promise<void> | null>(null);
  const sessionProviderRef = useRef(summaryPreferences.transcriptionProvider);
  const sessionAsrPreferencesRef = useRef(summaryPreferences);
  const finalizedAsrItemsRef = useRef(new Set<string>());
  const measuredAsrItemsRef = useRef(new Set<string>());
  const animationFrameRef = useRef<number | null>(null);
  const startedAtRef = useRef(0);
  const timerRef = useRef<number | null>(null);
  const persistTimerRef = useRef<number | null>(null);
  const persistenceGenerationRef = useRef(0);
  const demoTimeoutsRef = useRef<number[]>([]);
  const documentKeywordsRef = useRef<string[]>([]);
  const pendingTranslationsRef = useRef(new Set<Promise<void>>());
  const translationStartedAtRef = useRef(new Map<string, number>());
  const translationFirstTextAtRef = useRef(new Map<string, number>());
  const interimTranslationTimersRef = useRef(new Map<string, number>());
  const interimTranslationActiveRef = useRef(new Set<string>());

  const updateStatus = useCallback((next: SessionStatus) => {
    statusRef.current = next;
    setStatus(next);
  }, []);

  const updateSegments = useCallback(
    (updater: (current: TranscriptSegment[]) => TranscriptSegment[]) => {
      const next = updater(segmentsRef.current);
      segmentsRef.current = next;
      setSegments(next);
    },
    [],
  );

  const updateSummaries = useCallback(
    (updater: (current: TopicSummary[]) => TopicSummary[]) => {
      const next = updater(summariesRef.current);
      summariesRef.current = next;
      setSummaries(next);
    },
    [],
  );

  useEffect(() => {
    elapsedMsRef.current = elapsedMs;
  }, [elapsedMs]);

  const persistSnapshot = useCallback(async (overrideStatus?: string) => {
    const lectureId = lectureIdRef.current;
    if (!isTauri() || lectureId === null) return;
    await invoke("save_lecture_snapshot", {
      snapshot: {
        lectureId,
        elapsedMs: elapsedMsRef.current,
        status: overrideStatus ?? statusRef.current,
        segments: segmentsRef.current,
        summaries: summariesRef.current,
      },
    });
  }, []);

  const schedulePersistence = useCallback(() => {
    if (!isTauri() || lectureIdRef.current === null) return;
    if (persistTimerRef.current !== null) return;
    const generation = persistenceGenerationRef.current;
    persistTimerRef.current = window.setTimeout(() => {
      persistTimerRef.current = null;
      if (generation !== persistenceGenerationRef.current) return;
      void persistSnapshot().catch((reason) => setError(`自动保存失败：${String(reason)}`));
    }, 650);
  }, [persistSnapshot]);

  const cancelPendingPersistence = useCallback(() => {
    persistenceGenerationRef.current += 1;
    if (persistTimerRef.current !== null) {
      window.clearTimeout(persistTimerRef.current);
      persistTimerRef.current = null;
    }
  }, []);

  useEffect(() => {
    schedulePersistence();
  }, [segments, summaries, status, schedulePersistence]);

  const startTimer = useCallback(() => {
    startedAtRef.current = Date.now() - elapsedMsRef.current;
    if (timerRef.current !== null) window.clearInterval(timerRef.current);
    timerRef.current = window.setInterval(() => {
      const next = Date.now() - startedAtRef.current;
      elapsedMsRef.current = next;
      setElapsedMs(next);
    }, 500);
  }, []);

  const stopTimer = useCallback(() => {
    if (timerRef.current !== null) {
      window.clearInterval(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const startAudioMeter = useCallback((stream: MediaStream, forwardToStreamingAsr = false) => {
    const audioContext = new AudioContext();
    const analyser = audioContext.createAnalyser();
    analyser.fftSize = 256;
    const source = audioContext.createMediaStreamSource(stream);
    source.connect(analyser);
    audioSourceRef.current = source;
    audioContextRef.current = audioContext;

    if (forwardToStreamingAsr) {
      const processor = audioContext.createScriptProcessor(4_096, 1, 1);
      const silentGain = audioContext.createGain();
      silentGain.gain.value = 0;
      processor.onaudioprocess = (event) => {
        const provider = streamingAsrActiveRef.current;
        if (!provider) return;
        const pcm = downsampleToPcm16(
          event.inputBuffer.getChannelData(0),
          audioContext.sampleRate,
        );
        if (!pcm.length) return;
        asrAudioQueueRef.current = asrAudioQueueRef.current
          .then(() => invoke<void>(`send_${provider}_asr_audio`, {
            audioBase64: pcmToBase64(pcm),
          }))
          .catch((reason) => {
            if (streamingAsrActiveRef.current !== provider) return;
            streamingAsrActiveRef.current = null;
            setError(`实时转写音频发送失败：${String(reason)}`);
            stopTimer();
            updateStatus("error");
          });
      };
      source.connect(processor);
      processor.connect(silentGain);
      silentGain.connect(audioContext.destination);
      audioProcessorRef.current = processor;
      silentGainRef.current = silentGain;
    }
    const values = new Uint8Array(analyser.frequencyBinCount);

    const readLevel = () => {
      analyser.getByteFrequencyData(values);
      const average = values.reduce((total, value) => total + value, 0) / values.length;
      setAudioLevel(Math.min(1, average / 80));
      animationFrameRef.current = requestAnimationFrame(readLevel);
    };
    readLevel();
  }, [stopTimer, updateStatus]);

  const startStreamingAsr = useCallback(async (extraKeywords: string[] = []) => {
    await streamingAsrFinishRef.current.catch(() => undefined);
    const provider = sessionProviderRef.current;
    if (provider === "openai") return;
    asrAudioQueueRef.current = Promise.resolve();
    streamingAsrReceivingRef.current = provider;
    await invoke(`start_${provider}_asr`, {
      request: {
        model: sessionAsrPreferencesRef.current.transcriptionModel,
        elapsedOffsetMs: elapsedMsRef.current,
        hotwords: mergeAsrHotwords(settings.hotwords, extraKeywords),
        workspaceId: sessionAsrPreferencesRef.current.alibabaWorkspaceId,
      },
    });
    streamingAsrActiveRef.current = provider;
  }, [settings.hotwords]);

  const finishStreamingAsr = useCallback(() => {
    const provider = streamingAsrReceivingRef.current;
    if (!provider || streamingAsrFinishingRef.current) return streamingAsrFinishRef.current;
    streamingAsrFinishingRef.current = true;
    streamingAsrActiveRef.current = null;
    const finish = async () => {
      await asrAudioQueueRef.current.catch(() => undefined);
      try {
        await invoke(`finish_${provider}_asr`);
      } finally {
        streamingAsrReceivingRef.current = null;
        streamingAsrFinishingRef.current = false;
      }
    };
    streamingAsrFinishRef.current = finish();
    return streamingAsrFinishRef.current;
  }, []);

  const requestTranslation = useCallback(
    async (segmentId: string, english: string, interim = false) => {
      const completed = segmentsRef.current.filter(
        (segment) => segment.id !== segmentId && segment.state !== "interim",
      );
      const previousEnglish = completed[completed.length - 1]?.english ?? null;

      try {
        if (interimTranslationActiveRef.current.has(segmentId)) return;
        if (interim) interimTranslationActiveRef.current.add(segmentId);
        updateSegments((current) => replaceTranslation(current, segmentId, ""));
        translationStartedAtRef.current.set(segmentId, Date.now());
        await invoke("translate_segment", {
          request: {
            segmentId,
            english,
            previousEnglish,
            courseName: settings.courseName,
            glossary: settings.glossary,
            provider: summaryPreferences.translationProvider,
            workspaceId: summaryPreferences.alibabaWorkspaceId,
            model: summaryPreferences.translationProvider === "local"
              ? summaryPreferences.localTranslationModel
              : summaryPreferences.translationModel,
            localEndpoint: summaryPreferences.localTranslationEndpoint,
            openaiBaseUrl: summaryPreferences.openaiBaseUrl,
            keySlot: "translation",
          },
        });
      } catch (reason) {
        if (lectureIdRef.current === null && statusRef.current === "idle") return;
        updateSegments((current) => finishTranslation(current, segmentId, true));
        setError(String(reason));
      } finally {
        if (interim) interimTranslationActiveRef.current.delete(segmentId);
      }
    },
    [settings.courseName, settings.glossary, summaryPreferences, updateSegments],
  );

  const queueTranslation = useCallback((segmentId: string, english: string, interim = false) => {
    const task = requestTranslation(segmentId, english, interim);
    pendingTranslationsRef.current.add(task);
    void task.finally(() => pendingTranslationsRef.current.delete(task));
  }, [requestTranslation]);

  const waitForPendingTranslations = useCallback(async () => {
    const pending = [...pendingTranslationsRef.current];
    if (!pending.length) return true;
    let timeout: number | undefined;
    const completed = await Promise.race([
      Promise.allSettled(pending).then(() => true),
      new Promise<false>((resolve) => {
        timeout = window.setTimeout(() => resolve(false), 20_000);
      }),
    ]);
    if (timeout !== undefined) window.clearTimeout(timeout);
    return completed;
  }, []);

  const handleRealtimeEvent = useCallback(
    (event: Record<string, unknown>) => {
      const type = String(event.type ?? "");
      const itemId = String(event.item_id ?? "");
      const now = Math.max(0, Date.now() - startedAtRef.current);

      if (type === "conversation.item.input_audio_transcription.delta") {
        updateSegments((current) =>
          upsertTranscriptDelta(current, itemId, String(event.delta ?? ""), now),
        );
      }

      if (type === "conversation.item.input_audio_transcription.completed") {
        const transcript = String(event.transcript ?? "").trim();
        if (!transcript) return;
        updateSegments((current) => finalizeTranscript(current, itemId, transcript, now));
        queueTranslation(itemId, transcript);
        openAiFinalizationRef.current?.();
        openAiFinalizationRef.current = null;
      }

      if (type === "error") {
        const nestedError = event.error as Record<string, unknown> | undefined;
        setError(String(nestedError?.message ?? "实时转写出现错误"));
        updateStatus("error");
      }
    },
    [queueTranslation, updateSegments, updateStatus],
  );

  useEffect(() => {
    if (!isTauri()) return;

    const translationListener = listen<TranslationEvent>("translation-event", ({ payload }) => {
      if (lectureIdRef.current === null && statusRef.current === "idle") return;
      if (payload.kind === "delta") {
        const startedAt = translationStartedAtRef.current.get(payload.segmentId);
        if (startedAt !== undefined && !translationFirstTextAtRef.current.has(payload.segmentId)) {
          translationFirstTextAtRef.current.set(payload.segmentId, Date.now());
          setTranslationLatencyMs(Math.max(0, Date.now() - startedAt));
        }
        updateSegments((current) =>
          appendTranslation(current, payload.segmentId, payload.text),
        );
      } else {
        const startedAt = translationStartedAtRef.current.get(payload.segmentId);
        if (startedAt !== undefined && payload.kind === "done") {
          setTranslationTotalMs(Math.max(0, Date.now() - startedAt));
        }
        translationStartedAtRef.current.delete(payload.segmentId);
        translationFirstTextAtRef.current.delete(payload.segmentId);
        updateSegments((current) =>
          finishTranslation(current, payload.segmentId, payload.kind === "error"),
        );
        if (payload.kind === "error") setError(payload.text);
      }
    });
    const handleAsrEvent = (provider: StreamingProvider, payload: StreamingAsrEvent) => {
      if (streamingAsrReceivingRef.current !== provider || lectureIdRef.current === null) return;
      if (payload.kind !== "error" && !measuredAsrItemsRef.current.has(payload.itemId)) {
        measuredAsrItemsRef.current.add(payload.itemId);
        const now = Math.max(0, Date.now() - startedAtRef.current);
        setAsrLatencyMs(Math.max(0, Math.round(now - payload.startMs)));
      }
      if (payload.kind === "delta") {
        if (finalizedAsrItemsRef.current.has(payload.itemId)) return;
        updateSegments((current) =>
          discardSupersededInterims(
            upsertTranscriptText(current, payload.itemId, payload.text, payload.startMs),
            payload.itemId,
          ),
        );
        if (payload.text.trim().length >= 16 && !interimTranslationActiveRef.current.has(payload.itemId)) {
          const previousTimer = interimTranslationTimersRef.current.get(payload.itemId);
          if (previousTimer !== undefined) window.clearTimeout(previousTimer);
          const timer = window.setTimeout(() => {
            interimTranslationTimersRef.current.delete(payload.itemId);
            queueTranslation(payload.itemId, payload.text.trim(), true);
          }, 900);
          interimTranslationTimersRef.current.set(payload.itemId, timer);
        }
      } else if (payload.kind === "final") {
        const transcript = payload.text.trim();
        if (!transcript || finalizedAsrItemsRef.current.has(payload.itemId)) return;
        finalizedAsrItemsRef.current.add(payload.itemId);
        updateSegments((current) =>
          finalizeTranscript(current, payload.itemId, transcript, payload.startMs),
        );
        const interimTimer = interimTranslationTimersRef.current.get(payload.itemId);
        if (interimTimer !== undefined) window.clearTimeout(interimTimer);
        interimTranslationTimersRef.current.delete(payload.itemId);
        queueTranslation(payload.itemId, transcript);
      } else {
        streamingAsrActiveRef.current = null;
        setError(payload.text || "实时转写出现错误");
        stopTimer();
        updateStatus("error");
      }
    };
    const asrListeners = (["alibaba", "deepgram"] as const).map((provider) =>
      listen<StreamingAsrEvent>(`${provider}-asr-event`, ({ payload }) => handleAsrEvent(provider, payload)),
    );

    return () => {
      void translationListener.then((unlisten) => unlisten());
      asrListeners.forEach((listener) => void listener.then((unlisten) => unlisten()));
    };
  }, [queueTranslation, stopTimer, updateSegments, updateStatus]);

  const cleanupMedia = useCallback(() => {
    stopTimer();
    interimTranslationTimersRef.current.forEach((timer) => window.clearTimeout(timer));
    interimTranslationTimersRef.current.clear();
    interimTranslationActiveRef.current.clear();
    demoTimeoutsRef.current.forEach((timeout) => window.clearTimeout(timeout));
    demoTimeoutsRef.current = [];
    if (animationFrameRef.current !== null) {
      cancelAnimationFrame(animationFrameRef.current);
      animationFrameRef.current = null;
    }
    audioProcessorRef.current?.disconnect();
    audioSourceRef.current?.disconnect();
    silentGainRef.current?.disconnect();
    audioProcessorRef.current = null;
    audioSourceRef.current = null;
    silentGainRef.current = null;
    void finishStreamingAsr().catch(() => undefined);
    void audioContextRef.current?.close();
    audioContextRef.current = null;
    channelRef.current?.close();
    peerRef.current?.close();
    streamRef.current?.getTracks().forEach((track) => track.stop());
    channelRef.current = null;
    peerRef.current = null;
    streamRef.current = null;
    setAudioLevel(0);
  }, [finishStreamingAsr, stopTimer]);

  useEffect(() => cleanupMedia, [cleanupMedia]);

  const startLive = useCallback(async (options?: StartLectureOptions) => {
    if (!isTauri()) {
      setError("实时课堂需要从桌面应用启动");
      return null;
    }
    if (settings.courseId === null) {
      setError("请先创建并选择一门课程");
      return null;
    }

    const isResuming = lectureIdRef.current !== null;
    cleanupMedia();
    await streamingAsrFinishRef.current.catch(() => undefined);
    sessionProviderRef.current = summaryPreferences.transcriptionProvider;
    sessionAsrPreferencesRef.current = summaryPreferences;
    let documentKeywords: string[] = [];
    setError(null);
    updateStatus("connecting");

    if (!isResuming) {
      setSegments([]);
      segmentsRef.current = [];
      setSummaries([]);
      summariesRef.current = [];
      summaryCursorRef.current = 0;
      measuredAsrItemsRef.current.clear();
      finalizedAsrItemsRef.current.clear();
      documentKeywordsRef.current = [];
      pendingTranslationsRef.current.clear();
      setAsrLatencyMs(null);
      setTranslationLatencyMs(null);
      setTranslationTotalMs(null);
      translationStartedAtRef.current.clear();
      translationFirstTextAtRef.current.clear();
      setElapsedMs(0);
      elapsedMsRef.current = 0;
      startedAtRef.current = Date.now();
      try {
        lectureIdRef.current = await invoke<number>("begin_lecture", {
          lecture: {
            courseId: settings.courseId,
            title: options?.title?.trim()
              || `${settings.courseName} · ${new Date().toLocaleDateString("zh-CN")}`,
            startedAt: Date.now(),
          },
        });
        setLectureId(lectureIdRef.current);
        if (options?.slides && lectureIdRef.current !== null) {
          await invoke("import_lecture_slides", {
            request: {
              courseId: settings.courseId,
              lectureId: lectureIdRef.current,
              ...options.slides,
            },
          });
        }
      } catch (reason) {
        if (!isResuming && lectureIdRef.current !== null) {
          await invoke("finish_lecture", {
            lectureId: lectureIdRef.current,
            elapsedMs: elapsedMsRef.current,
            endedAt: Date.now(),
          }).then(() => invoke("delete_lecture", { lectureId: lectureIdRef.current }))
            .catch(() => undefined);
          lectureIdRef.current = null;
          setLectureId(null);
        }
        updateStatus("error");
        setError(`无法创建课堂记录：${String(reason)}`);
        return null;
      }
    } else {
      startedAtRef.current = Date.now() - elapsedMsRef.current;
    }

    if (lectureIdRef.current !== null) {
      documentKeywords = await invoke<string[]>("list_lecture_document_keywords", {
        lectureId: lectureIdRef.current,
      }).catch(() => []);
      documentKeywordsRef.current = documentKeywords;
    }

    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
          channelCount: 1,
          ...(summaryPreferences.microphoneDeviceId
            ? { deviceId: { exact: summaryPreferences.microphoneDeviceId } }
            : {}),
        },
      });
      streamRef.current = stream;
      if (sessionProviderRef.current !== "openai") {
        await startStreamingAsr(documentKeywords);
        startAudioMeter(stream, true);
        updateStatus("live");
        startTimer();
        return lectureIdRef.current;
      }

      startAudioMeter(stream);

      const peer = new RTCPeerConnection();
      peerRef.current = peer;
      stream.getAudioTracks().forEach((track) => peer.addTrack(track, stream));

      const channel = peer.createDataChannel("oai-events");
      channelRef.current = channel;
      channel.addEventListener("message", ({ data }) => {
        try {
          handleRealtimeEvent(JSON.parse(String(data)) as Record<string, unknown>);
        } catch {
          setError("收到无法解析的实时事件");
        }
      });
      channel.addEventListener("open", () => {
        updateStatus("live");
        startTimer();
      });

      peer.addEventListener("connectionstatechange", () => {
        if (["failed", "disconnected"].includes(peer.connectionState)) {
          updateStatus("error");
          setError("实时连接已中断，已保留当前课堂记录");
        }
      });

      const offer = await peer.createOffer();
      await peer.setLocalDescription(offer);
      const answerSdp = await invoke<string>("create_realtime_call", {
        sdp: offer.sdp ?? "",
        config: {
          ...settings,
          keywords: [...new Set([...settings.keywords, ...documentKeywords])].slice(0, 200),
          transcriptionModel: summaryPreferences.transcriptionModel,
        },
      });
      await peer.setRemoteDescription({ type: "answer", sdp: answerSdp });
      return lectureIdRef.current;
    } catch (reason) {
      cleanupMedia();
      updateStatus("error");
      setError(String(reason));
      await persistSnapshot("error").catch(() => undefined);
      return null;
    }
  }, [cleanupMedia, handleRealtimeEvent, persistSnapshot, settings, startStreamingAsr, startAudioMeter, startTimer, summaryPreferences, updateStatus]);

  const restoreLecture = useCallback(
    (snapshot: LectureSnapshot) => {
      cleanupMedia();
      lectureIdRef.current = snapshot.lectureId;
      setLectureId(snapshot.lectureId);
      segmentsRef.current = snapshot.segments;
      summariesRef.current = snapshot.summaries;
      const summarizedThrough = snapshot.summaries
        .filter((summary) => summary.kind !== "lecture")
        .reduce((latest, summary) => Math.max(latest, summary.endMs ?? summary.startMs), -1);
      summaryCursorRef.current = summarizedThrough < 0
        ? 0
        : snapshot.segments.reduce(
            (cursor, segment, index) => segment.startMs <= summarizedThrough ? index + 1 : cursor,
            0,
          );
      elapsedMsRef.current = snapshot.elapsedMs;
      startedAtRef.current = Date.now() - snapshot.elapsedMs;
      setSegments(snapshot.segments);
      setSummaries(snapshot.summaries);
      setElapsedMs(snapshot.elapsedMs);
      updateStatus("recovered");
    },
    [cleanupMedia, updateStatus],
  );

  const discardRecovered = useCallback(async () => {
    const lectureId = lectureIdRef.current;
    cleanupMedia();
    if (isTauri() && lectureId !== null) {
      try {
        await persistSnapshot("ending");
        await invoke("finish_lecture", {
          lectureId,
          elapsedMs: elapsedMsRef.current,
          endedAt: Date.now(),
        });
      } catch (reason) {
        setError(`无法结束已恢复的课堂记录：${String(reason)}`);
        updateStatus("error");
        return;
      }
    }
    lectureIdRef.current = null;
    pendingTranslationsRef.current.clear();
    setLectureId(null);
    segmentsRef.current = [];
    summariesRef.current = [];
    summaryCursorRef.current = 0;
    elapsedMsRef.current = 0;
    setSegments([]);
    setSummaries([]);
    setElapsedMs(0);
    updateStatus("idle");
  }, [cleanupMedia, persistSnapshot, updateStatus]);

  const startDemo = useCallback(() => {
    if (lectureIdRef.current !== null) return;
    cleanupMedia();
    setError(null);
    setSegments([]);
    segmentsRef.current = [];
    setSummaries([]);
    summariesRef.current = [];
    summaryCursorRef.current = 0;
    setElapsedMs(0);
    elapsedMsRef.current = 0;
    startedAtRef.current = Date.now();
    updateStatus("demo");
    startTimer();

    demoSegments.forEach((sample, index) => {
      const baseDelay = index * 1900;
      const itemId = `demo-${index}`;
      demoTimeoutsRef.current.push(
        window.setTimeout(() => {
          const startMs = Date.now() - startedAtRef.current;
          updateSegments((current) =>
            upsertTranscriptDelta(current, itemId, sample.english.slice(0, 38), startMs),
          );
        }, baseDelay + 300),
        window.setTimeout(() => {
          const startMs = Date.now() - startedAtRef.current;
          updateSegments((current) =>
            finalizeTranscript(current, itemId, sample.english, startMs),
          );
        }, baseDelay + 800),
        window.setTimeout(() => {
          updateSegments((current) =>
            finishTranslation(appendTranslation(current, itemId, sample.chinese), itemId),
          );
          if (index === 3) {
            updateSummaries(() => [
              formatDemoSummary(Date.now() - startedAtRef.current),
            ]);
          }
        }, baseDelay + 1350),
      );
    });
  }, [cleanupMedia, startTimer, updateSegments, updateStatus, updateSummaries]);

  const togglePause = useCallback(async () => {
    if (endingRef.current || pauseTransitionRef.current
      || !streamRef.current || !["live", "paused"].includes(statusRef.current)) return;
    const stream = streamRef.current;
    const shouldPause = statusRef.current === "live";
    const transition = async () => {
      try {
        if (shouldPause) {
          stream.getAudioTracks().forEach((track) => {
            track.enabled = false;
          });
          stopTimer();
          await finishStreamingAsr();
          if (endingRef.current) return;
          updateStatus("paused");
          await persistSnapshot("paused");
        } else {
          if (sessionProviderRef.current !== "openai") {
            await startStreamingAsr(documentKeywordsRef.current);
          }
          if (endingRef.current) return;
          stream.getAudioTracks().forEach((track) => {
            track.enabled = true;
          });
          startTimer();
          updateStatus("live");
        }
      } catch (reason) {
        setError(String(reason));
        if (!endingRef.current) updateStatus("error");
      }
    };
    pauseTransitionRef.current = transition();
    try {
      await pauseTransitionRef.current;
    } finally {
      pauseTransitionRef.current = null;
    }
  }, [finishStreamingAsr, persistSnapshot, startStreamingAsr, startTimer, stopTimer, updateStatus]);

  const generateSummary = useCallback(
    async (automatic = false, kind: "topic" | "lecture" = "topic") => {
      if (summaryInFlightRef.current) return;

      if (statusRef.current === "demo") {
        updateSummaries((current) => {
          const withoutExistingLecture = kind === "lecture"
            ? current.filter((summary) => summary.kind !== "lecture")
            : current;
          return [...withoutExistingLecture, formatDemoSummary(elapsedMsRef.current, kind)];
        });
        return;
      }
      if (!isTauri()) {
        setError("模型总结需要从桌面应用启动");
        return;
      }

      const allSegments = segmentsRef.current;
      let candidates = allSegments
        .slice(kind === "lecture" ? 0 : summaryCursorRef.current)
        .filter((segment) => segment.state !== "interim" && segment.english.trim());
      if (!candidates.length && !automatic) {
        candidates = allSegments
          .filter((segment) => segment.state !== "interim" && segment.english.trim())
          .slice(-5);
      }
      if (!candidates.length) {
        setError("还没有足够的稳定课堂内容可供总结");
        return;
      }

      summaryInFlightRef.current = true;
      setSummaryStatus(summaryPreferences.webSearchEnabled ? "searching" : "generating");
      const summaryId = `${kind}-${automatic ? "auto" : "manual"}-${Date.now()}`;
      const lastCandidate = candidates[candidates.length - 1];
      const transcript = kind === "lecture"
        ? buildLectureSummaryInput(candidates, summariesRef.current)
        : buildSummaryTranscript(candidates);

      try {
        const summary = await invoke<TopicSummary>("generate_topic_summary", {
          request: {
            summaryId,
            kind,
            startMs: candidates[0].startMs,
            endMs: lastCandidate.startMs,
            courseName: settings.courseName,
            courseContext: settings.courseContext,
            glossary: settings.glossary,
            transcript,
            provider: summaryPreferences.textProvider,
            workspaceId: summaryPreferences.alibabaWorkspaceId,
            model: summaryModelForKind(summaryPreferences, kind),
            webSearch: summaryPreferences.webSearchEnabled,
            openaiBaseUrl: summaryPreferences.openaiBaseUrl,
            keySlot: kind === "lecture" ? "lecture-summary" : "summary",
          },
        });
        updateSummaries((current) => {
          const withoutExistingLecture = kind === "lecture"
            ? current.filter((item) => item.kind !== "lecture")
            : current;
          return [...withoutExistingLecture, summary];
        });
        if (kind === "topic") {
          const lastIndex = allSegments.findIndex((segment) => segment.id === lastCandidate.id);
          if (lastIndex >= summaryCursorRef.current) summaryCursorRef.current = lastIndex + 1;
        }
      } catch (reason) {
        setError(`${kind === "lecture" ? "整课" : "阶段"}总结失败：${String(reason)}`);
      } finally {
        summaryInFlightRef.current = false;
        setSummaryStatus("idle");
      }
    },
    [settings, summaryPreferences, updateSummaries],
  );

  useEffect(() => {
    if (status !== "live" || !summaryPreferences.autoSummaryEnabled) return;
    const pending = segments
      .slice(summaryCursorRef.current)
      .filter((segment) => ["complete", "error"].includes(segment.state));
    if (shouldAutoSummarize(pending)) void generateSummary(true, "topic");
  }, [generateSummary, segments, status, summaryPreferences.autoSummaryEnabled]);

  const stop = useCallback(async (generateLectureSummary = true) => {
    if (endingRef.current) return;
    endingRef.current = true;
    setIsEnding(true);
    cancelPendingPersistence();
    const lectureId = lectureIdRef.current;
    await pauseTransitionRef.current;
    if (
      sessionProviderRef.current === "openai"
      && statusRef.current === "live"
      && streamRef.current
    ) {
      streamRef.current.getAudioTracks().forEach((track) => {
        track.enabled = false;
      });
      const finalized = new Promise<void>((resolve) => {
        openAiFinalizationRef.current = resolve;
      });
      channelRef.current?.send(JSON.stringify({ type: "input_audio_buffer.commit" }));
      await Promise.race([
        finalized,
        new Promise<void>((resolve) => window.setTimeout(resolve, 3_000)),
      ]);
    }
    await finishStreamingAsr().catch((reason) => setError(String(reason)));
    cleanupMedia();
    try {
      const translationsCompleted = await waitForPendingTranslations();
      let finalSegments = segmentsRef.current.filter((segment) => segment.state !== "interim");
      if (!translationsCompleted) {
        finalSegments = finalSegments.map((segment) =>
          segment.state === "translating" ? { ...segment, state: "error" } : segment,
        );
        setError("部分中文翻译在结束前超时，英文转写已完整保存");
      }
      segmentsRef.current = finalSegments;
      setSegments(finalSegments);
      if (generateLectureSummary && segmentsRef.current.some((segment) => segment.state !== "interim" && segment.english.trim())) {
        const summaryWaitStartedAt = Date.now();
        while (summaryInFlightRef.current && Date.now() - summaryWaitStartedAt < 30_000) {
          await new Promise((resolve) => window.setTimeout(resolve, 100));
        }
        await generateSummary(false, "lecture");
      }
      if (isTauri() && lectureId !== null) {
        try {
          await persistSnapshot("ending");
          await invoke("finish_lecture", {
            lectureId,
            elapsedMs: elapsedMsRef.current,
            endedAt: Date.now(),
          });
        } catch (reason) {
          await persistSnapshot("error").catch(() => undefined);
          setError(`课堂内容尚未完成最终保存，可点击“继续听课”后重试：${String(reason)}`);
          updateStatus("error");
          return;
        }
      }
      lectureIdRef.current = null;
      pendingTranslationsRef.current.clear();
      setLectureId(null);
      updateStatus("idle");
    } finally {
      endingRef.current = false;
      setIsEnding(false);
    }
  }, [cancelPendingPersistence, cleanupMedia, finishStreamingAsr, generateSummary, persistSnapshot, updateStatus, waitForPendingTranslations]);

  return {
    status,
    segments,
    summaries,
    elapsedMs,
    audioLevel,
    asrLatencyMs,
    translationLatencyMs,
    translationTotalMs,
    error,
    summaryStatus,
    isSummarizing: summaryStatus !== "idle",
    isEnding,
    hasRecoveredLecture: status === "recovered",
    lectureId,
    clearError: () => setError(null),
    startLive,
    startDemo,
    togglePause,
    stop,
    addManualSummary: () => void generateSummary(false, "topic"),
    restoreLecture,
    discardRecovered,
  };
}
