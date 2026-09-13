import { describe, expect, it } from "vitest";
import type { SummaryPreferences } from "../types";
import {
  ASR_CONFIG_VERSION,
  DEEPGRAM_ASR_MODEL,
  LEGACY_QWEN3_ASR_REALTIME_MODEL,
  LOCAL_TRANSLATION_MODEL,
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
  translationProvider: "alibaba",
  alibabaWorkspaceId: "",
  translationModel: "qwen-mt-lite",
  localTranslationModel: "translategemma:4b",
  localTranslationEndpoint: "http://127.0.0.1:11434/v1",
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

  it.each([DEEPGRAM_ASR_MODEL, "nova-2"])("preserves the selected Deepgram model %s", (model) => {
    const saved: SummaryPreferences = {
      ...preferences,
      asrConfigVersion: 0,
      transcriptionProvider: "deepgram",
      transcriptionModel: model,
    };

    expect(migrateAsrPreferences(saved, saved)).toEqual({
      ...saved,
      asrConfigVersion: ASR_CONFIG_VERSION,
    });
  });

  it("preserves an OpenAI selection while migrating legacy ASR settings", () => {
    const saved: SummaryPreferences = {
      ...preferences,
      asrConfigVersion: 0,
      transcriptionProvider: "openai",
      transcriptionModel: "gpt-live-transcribe",
    };

    expect(migrateAsrPreferences(saved, saved)).toEqual({
      ...saved,
      asrConfigVersion: ASR_CONFIG_VERSION,
    });
  });

  it("upgrades legacy Alibaba text models without changing Deepgram ASR", () => {
    const saved: SummaryPreferences = {
      ...preferences,
      transcriptionProvider: "deepgram",
      transcriptionModel: DEEPGRAM_ASR_MODEL,
      textConfigVersion: 0,
      translationModel: "qwen3.5-flash",
      summaryModel: "qwen3.5-flash",
      lectureSummaryModel: "",
    };

    expect(migrateTextPreferences(migrateAsrPreferences(saved, saved), saved)).toMatchObject({
      ...TEXT_MODEL_DEFAULTS.alibaba,
      transcriptionProvider: "deepgram",
      transcriptionModel: DEEPGRAM_ASR_MODEL,
    });
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

  it("keeps translation on the former text provider when migrating", () => {
    const legacy = { ...preferences, textConfigVersion: 3 } as SummaryPreferences;
    delete (legacy as Partial<SummaryPreferences>).translationProvider;
    delete (legacy as Partial<SummaryPreferences>).localTranslationModel;
    delete (legacy as Partial<SummaryPreferences>).localTranslationEndpoint;

    expect(migrateTextPreferences(legacy, legacy)).toMatchObject({
      translationProvider: "alibaba",
      localTranslationModel: LOCAL_TRANSLATION_MODEL,
      localTranslationEndpoint: "http://127.0.0.1:11434/v1",
    });
  });

  it("normalizes unknown provider values from persisted preferences", () => {
    const malformed = {
      ...preferences,
      textProvider: "untrusted-provider",
      translationProvider: "untrusted-provider",
      transcriptionProvider: "untrusted-provider",
      localTranslationModel: 42,
      localTranslationEndpoint: null,
    } as unknown as SummaryPreferences;

    const migratedAsr = migrateAsrPreferences(malformed, malformed);
    const migrated = migrateTextPreferences(migratedAsr, malformed);

    expect(migrated).toMatchObject({
      textProvider: "openai",
      translationProvider: "openai",
      transcriptionProvider: "openai",
      localTranslationModel: LOCAL_TRANSLATION_MODEL,
      localTranslationEndpoint: "http://127.0.0.1:11434/v1",
    });
  });

  it("retains an explicitly selected local translation provider", () => {
    const local: SummaryPreferences = {
      ...preferences,
      textProvider: "alibaba",
      translationProvider: "local",
    };

    expect(migrateTextPreferences(local, local)).toMatchObject({
      textProvider: "alibaba",
      translationProvider: "local",
      localTranslationModel: "translategemma:4b",
    });
  });

  it("preserves a custom model after migration", () => {
    const custom = { ...preferences, lectureSummaryModel: "qwen3.8-max" };

    expect(migrateTextPreferences(custom, custom).lectureSummaryModel).toBe("qwen3.8-max");
  });

  it("uses the dedicated model for a lecture summary", () => {
    expect(summaryModelForKind(preferences, "topic")).toBe("qwen3.7-flash");
    expect(summaryModelForKind(preferences, "lecture")).toBe("qwen3.7-plus");
  });
});
