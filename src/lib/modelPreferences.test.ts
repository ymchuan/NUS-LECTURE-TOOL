import { describe, expect, it } from "vitest";
import type { SummaryPreferences } from "../types";
import {
  ASR_CONFIG_VERSION,
  LEGACY_QWEN3_ASR_REALTIME_MODEL,
  migrateAsrPreferences,
  migrateTextPreferences,
  QWEN3_ASR_REALTIME_MODEL,
  summaryModelForKind,
  TEXT_MODEL_DEFAULTS,
} from "./modelPreferences";

const preferences: SummaryPreferences = {
  asrConfigVersion: ASR_CONFIG_VERSION,
  textConfigVersion: 3,
  autoSummaryEnabled: true,
  webSearchEnabled: true,
  transcriptionProvider: "alibaba",
  transcriptionModel: QWEN3_ASR_REALTIME_MODEL,
  microphoneDeviceId: "",
  textProvider: "alibaba",
  alibabaWorkspaceId: "",
  openaiBaseUrl: "https://api.openai.com/v1",
  ollamaBaseUrl: "http://127.0.0.1:11434",
  ollamaTranslationModel: "qwen2.5:3b",
  ollamaCloudTranslationModel: "",
  translationModel: "qwen-mt-lite",
  summaryModel: "qwen3.7-flash",
  lectureSummaryModel: "qwen3.7-plus",
};

describe("model preferences", () => {
  it("moves the former Alibaba default to the accent-optimized realtime model", () => {
    const legacy = {
      ...preferences,
      asrConfigVersion: 2,
      transcriptionModel: "qwen-audio-3.0-asr-flash-streaming",
    };

    expect(migrateAsrPreferences(legacy, legacy)).toMatchObject({
      asrConfigVersion: ASR_CONFIG_VERSION,
      transcriptionModel: QWEN3_ASR_REALTIME_MODEL,
    });
  });

  it("moves the old Qwen3 alias to the latest accuracy snapshot", () => {
    const legacy = {
      ...preferences,
      asrConfigVersion: 3,
      transcriptionModel: LEGACY_QWEN3_ASR_REALTIME_MODEL,
    };

    expect(migrateAsrPreferences(legacy, legacy).transcriptionModel)
      .toBe(QWEN3_ASR_REALTIME_MODEL);
  });

  it("preserves an explicitly selected current ASR model", () => {
    expect(migrateAsrPreferences(preferences, preferences).transcriptionModel)
      .toBe(QWEN3_ASR_REALTIME_MODEL);
  });

  it("upgrades the legacy Alibaba text preset once", () => {
    const legacy = {
      ...preferences,
      textConfigVersion: 0,
      translationModel: "qwen3.5-flash",
      summaryModel: "qwen3.5-flash",
      lectureSummaryModel: "",
    };

    expect(migrateTextPreferences(legacy, legacy)).toMatchObject(TEXT_MODEL_DEFAULTS.alibaba);
  });

  it("preserves a custom model after migration", () => {
    const custom = { ...preferences, lectureSummaryModel: "qwen3.8-max" };

    expect(migrateTextPreferences(custom, custom).lectureSummaryModel).toBe("qwen3.8-max");
  });

  it("moves the old global Ollama profile to safe cloud defaults", () => {
    const legacy = {
      ...preferences,
      textConfigVersion: 5,
      textProvider: "ollama" as const,
      translationModel: "qwen2.5:3b",
      summaryModel: "qwen2.5:3b",
      lectureSummaryModel: "qwen2.5:3b",
      ollamaTranslationModel: "qwen2.5:7b",
      autoSummaryEnabled: true,
    };

    expect(migrateTextPreferences(legacy, legacy)).toMatchObject({
      textConfigVersion: 6,
      textProvider: "openai",
      ...TEXT_MODEL_DEFAULTS.openai,
      ollamaTranslationModel: "qwen2.5:7b",
      autoSummaryEnabled: false,
    });
  });

  it("does not rewrite a v5 user's custom 5.4 models during the version bump", () => {
    const custom = {
      ...preferences,
      textConfigVersion: 5,
      textProvider: "openai" as const,
      translationModel: "gpt-5.4",
      summaryModel: "gpt-5.4",
      lectureSummaryModel: "gpt-5.4",
    };

    expect(migrateTextPreferences(custom, custom)).toMatchObject({
      textConfigVersion: 6,
      translationModel: "gpt-5.4",
      summaryModel: "gpt-5.4",
      lectureSummaryModel: "gpt-5.4",
    });
  });

  it("preserves current cloud settings and independent retry models", () => {
    const custom = {
      ...preferences,
      textConfigVersion: 5,
      translationModel: "custom-translation",
      summaryModel: "custom-topic",
      lectureSummaryModel: "custom-lecture",
      ollamaTranslationModel: "qwen2.5:7b",
      ollamaCloudTranslationModel: "custom-ollama-cloud",
    };
    const original = { ...custom };

    expect(migrateTextPreferences(custom, custom)).toEqual({ ...original, textConfigVersion: 6 });
    expect(custom).toEqual(original);
  });

  it("uses the former global Ollama model when no independent retry model was saved", () => {
    const legacy = {
      ...preferences,
      textConfigVersion: 5,
      textProvider: "ollama" as const,
      translationModel: "qwen2.5:7b",
    };
    const saved = { ...legacy, ollamaTranslationModel: undefined };

    expect(migrateTextPreferences(legacy, saved)).toMatchObject({
      textProvider: "openai",
      ollamaTranslationModel: "qwen2.5:7b",
      autoSummaryEnabled: false,
    });
  });

  it("normalizes the old OpenAI nano/mini aliases", () => {
    const legacy = {
      ...preferences,
      textProvider: "openai" as const,
      textConfigVersion: 3,
      translationModel: "gpt-5.4-nano",
      summaryModel: "gpt-5.4-mini",
      lectureSummaryModel: "gpt-5.4-mini",
    };

    expect(migrateTextPreferences(legacy, legacy)).toMatchObject({
      translationModel: "gpt-5.6-luna",
      summaryModel: "gpt-5.6-luna",
      lectureSummaryModel: "gpt-5.6-terra",
    });
  });

  it("uses the dedicated model for a lecture summary", () => {
    expect(summaryModelForKind(preferences, "topic")).toBe("qwen3.7-flash");
    expect(summaryModelForKind(preferences, "lecture")).toBe("qwen3.7-plus");
  });
});
