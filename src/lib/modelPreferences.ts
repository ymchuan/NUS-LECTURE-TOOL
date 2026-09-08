import type {
  ModelProvider,
  SummaryPreferences,
  TranscriptionProvider,
  TranslationProvider,
} from "../types";

export const ASR_CONFIG_VERSION = 4;
export const TEXT_CONFIG_VERSION = 4;
export const DEEPGRAM_ASR_MODEL = "nova-3";
export const LOCAL_TRANSLATION_MODEL = "qwen3:4b-instruct-2507-q4_K_M";
export const LOCAL_TRANSLATION_ENDPOINT = "http://127.0.0.1:11434/v1";
export const QWEN3_ASR_REALTIME_MODEL = "qwen3-asr-flash-realtime-2026-02-10";
export const LEGACY_QWEN3_ASR_REALTIME_MODEL = "qwen3-asr-flash-realtime";
export const QWEN_AUDIO_STREAMING_MODEL = "qwen-audio-3.0-asr-flash-streaming";

function isModelProvider(value: unknown): value is ModelProvider {
  return value === "openai" || value === "alibaba";
}

function isTranslationProvider(value: unknown): value is TranslationProvider {
  return isModelProvider(value) || value === "local";
}

function isTranscriptionProvider(value: unknown): value is TranscriptionProvider {
  return isModelProvider(value) || value === "deepgram";
}

export function usesAlibabaSingaporeAsr(model: string): boolean {
  return model === QWEN3_ASR_REALTIME_MODEL
    || model === LEGACY_QWEN3_ASR_REALTIME_MODEL
    || model === QWEN_AUDIO_STREAMING_MODEL;
}

export function migrateAsrPreferences(
  preferences: SummaryPreferences,
  saved: Partial<SummaryPreferences>,
): SummaryPreferences {
  const next = { ...preferences, asrConfigVersion: ASR_CONFIG_VERSION };
  next.transcriptionProvider = isTranscriptionProvider(next.transcriptionProvider)
    ? next.transcriptionProvider
    : "openai";
  const savedVersion = Number(saved.asrConfigVersion ?? 0);
  if (savedVersion < 2
    && next.transcriptionProvider === "alibaba"
    && next.transcriptionModel === "paraformer-realtime-v2") {
    next.transcriptionModel = QWEN_AUDIO_STREAMING_MODEL;
  }
  if (savedVersion < ASR_CONFIG_VERSION
    && next.transcriptionProvider === "alibaba"
    && (next.transcriptionModel === QWEN_AUDIO_STREAMING_MODEL
      || next.transcriptionModel === LEGACY_QWEN3_ASR_REALTIME_MODEL)) {
    next.transcriptionModel = QWEN3_ASR_REALTIME_MODEL;
  }
  return next;
}

export const TEXT_MODEL_DEFAULTS: Record<
  ModelProvider,
  Pick<SummaryPreferences, "translationModel" | "summaryModel" | "lectureSummaryModel">
> = {
  openai: {
    translationModel: "gpt-5.4-nano",
    summaryModel: "gpt-5.4-mini",
    lectureSummaryModel: "gpt-5.4-mini",
  },
  alibaba: {
    translationModel: "qwen-mt-lite",
    summaryModel: "qwen3.7-flash",
    lectureSummaryModel: "qwen3.7-plus",
  },
};

export function migrateTextPreferences(
  preferences: SummaryPreferences,
  saved: Partial<SummaryPreferences>,
): SummaryPreferences {
  const next = { ...preferences, textConfigVersion: TEXT_CONFIG_VERSION };
  const isLegacy = Number(saved.textConfigVersion ?? 0) < TEXT_CONFIG_VERSION;

  next.textProvider = isModelProvider(next.textProvider) ? next.textProvider : "openai";
  next.translationProvider = isTranslationProvider(saved.translationProvider)
    ? saved.translationProvider
    : next.textProvider;
  if (typeof saved.localTranslationModel !== "string" || !saved.localTranslationModel.trim()) {
    next.localTranslationModel = LOCAL_TRANSLATION_MODEL;
  }
  if (typeof saved.localTranslationEndpoint !== "string" || !saved.localTranslationEndpoint.trim()) {
    next.localTranslationEndpoint = LOCAL_TRANSLATION_ENDPOINT;
  }

  if (isLegacy && next.textProvider === "alibaba") {
    if (
      !saved.translationModel
      || saved.translationModel === "qwen3.5-flash"
      || saved.translationModel === "qwen3.7-flash"
    ) {
      next.translationModel = TEXT_MODEL_DEFAULTS.alibaba.translationModel;
    }
    if (!saved.summaryModel || saved.summaryModel === "qwen3.5-flash") {
      next.summaryModel = TEXT_MODEL_DEFAULTS.alibaba.summaryModel;
    }
  }

  if (!saved.lectureSummaryModel) {
    next.lectureSummaryModel = isLegacy && next.textProvider === "openai"
      ? next.summaryModel
      : TEXT_MODEL_DEFAULTS[next.textProvider].lectureSummaryModel;
  }

  return next;
}

export function summaryModelForKind(
  preferences: SummaryPreferences,
  kind: "topic" | "lecture",
) {
  return kind === "lecture" ? preferences.lectureSummaryModel : preferences.summaryModel;
}
