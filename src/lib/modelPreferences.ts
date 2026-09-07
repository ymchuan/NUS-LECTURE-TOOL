import type { ModelProvider, SummaryPreferences } from "../types";

export const ASR_CONFIG_VERSION = 4;
export const TEXT_CONFIG_VERSION = 6;
export const QWEN3_ASR_REALTIME_MODEL = "qwen3-asr-flash-realtime-2026-02-10";
export const LEGACY_QWEN3_ASR_REALTIME_MODEL = "qwen3-asr-flash-realtime";
export const QWEN_AUDIO_STREAMING_MODEL = "qwen-audio-3.0-asr-flash-streaming";
export const DEEPGRAM_NOVA_MODEL = "nova-3";

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
    translationModel: "gpt-5.6-luna",
    summaryModel: "gpt-5.6-luna",
    lectureSummaryModel: "gpt-5.6-terra",
  },
  alibaba: {
    translationModel: "qwen-mt-lite",
    summaryModel: "qwen3.7-flash",
    lectureSummaryModel: "qwen3.7-plus",
  },
  ollama: {
    translationModel: "qwen2.5:3b",
    summaryModel: "qwen2.5:3b",
    lectureSummaryModel: "qwen2.5:3b",
  },
};

export function migrateTextPreferences(
  preferences: SummaryPreferences,
  saved: Partial<SummaryPreferences>,
): SummaryPreferences {
  const next = {
    ...preferences,
    textConfigVersion: TEXT_CONFIG_VERSION,
    ollamaCloudTranslationModel: typeof saved.ollamaCloudTranslationModel === "string"
      ? saved.ollamaCloudTranslationModel : preferences.ollamaCloudTranslationModel ?? "",
  };
  const isLegacy = Number(saved.textConfigVersion ?? 0) < 5;

  if (next.textProvider === "ollama") {
    // Move local models to explicit retries without enabling paid automatic summaries.
    const localModel = [saved.ollamaTranslationModel, saved.translationModel, next.ollamaTranslationModel]
      .find((model) => typeof model === "string" && model.trim());
    next.ollamaTranslationModel = localModel ?? TEXT_MODEL_DEFAULTS.ollama.translationModel;
    next.textProvider = "openai";
    Object.assign(next, TEXT_MODEL_DEFAULTS.openai);
    next.autoSummaryEnabled = false;
  }

  // Earlier builds used unverified `gpt-5.4-nano`/`gpt-5.4-mini` aliases.
  // Normalize those defaults to the model name exposed by compatible gateways.
  if (isLegacy && next.textProvider === "openai") {
    if (next.translationModel === "gpt-5.4-nano" || next.translationModel === "gpt-5.4-mini") {
      next.translationModel = TEXT_MODEL_DEFAULTS.openai.translationModel;
    }
    if (next.summaryModel === "gpt-5.4-nano" || next.summaryModel === "gpt-5.4-mini") {
      next.summaryModel = TEXT_MODEL_DEFAULTS.openai.summaryModel;
    }
    if (next.lectureSummaryModel === "gpt-5.4-nano" || next.lectureSummaryModel === "gpt-5.4-mini") {
      next.lectureSummaryModel = TEXT_MODEL_DEFAULTS.openai.lectureSummaryModel;
    }
    if (saved.summaryModel === "gpt-5.4") {
      next.summaryModel = TEXT_MODEL_DEFAULTS.openai.summaryModel;
    }
    if (saved.translationModel === "gpt-5.4") {
      next.translationModel = TEXT_MODEL_DEFAULTS.openai.translationModel;
    }
    if (saved.lectureSummaryModel === "gpt-5.4") {
      next.lectureSummaryModel = TEXT_MODEL_DEFAULTS.openai.lectureSummaryModel;
    }
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
