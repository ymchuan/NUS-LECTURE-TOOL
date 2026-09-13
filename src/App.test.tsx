// @vitest-environment jsdom

import { invoke } from "@tauri-apps/api/core";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CourseDialog } from "./App";
import type { SummaryPreferences } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const preferences: SummaryPreferences = {
  asrConfigVersion: 4,
  textConfigVersion: 4,
  autoSummaryEnabled: true,
  webSearchEnabled: true,
  transcriptionProvider: "deepgram",
  transcriptionModel: "nova-3",
  microphoneDeviceId: "",
  textProvider: "openai",
  translationProvider: "openai",
  alibabaWorkspaceId: "",
  translationModel: "gpt-5.4-nano",
  localTranslationModel: "translategemma:4b",
  localTranslationEndpoint: "http://127.0.0.1:11434/v1",
  summaryModel: "gpt-5.4-mini",
  lectureSummaryModel: "gpt-5.4-mini",
};

describe("Deepgram settings", () => {
  let container: HTMLDivElement;
  let root: Root;

  async function renderDialog(hasDeepgramApiKey: boolean) {
    await act(async () => {
      root.render(
        <CourseDialog
          open
          courses={[]}
          initialCourse={null}
          initialTerms={[]}
          initialSummaryPreferences={preferences}
          hasOpenAiApiKey
          hasAlibabaApiKey={false}
          hasAlibabaAsrApiKey={false}
          hasDeepgramApiKey={hasDeepgramApiKey}
          courseLocked={false}
          onClose={vi.fn()}
          onLoadCourse={vi.fn()}
          onSave={vi.fn()}
          onDeleteKey={vi.fn()}
        />,
      );
    });
  }

  beforeEach(async () => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean })
      .IS_REACT_ACT_ENVIRONMENT = true;
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    await renderDialog(true);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    delete (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    vi.clearAllMocks();
  });

  it("tests the selected Nova model using the saved Keychain credential", async () => {
    vi.mocked(invoke).mockResolvedValue({ model: "nova-3", elapsedMs: 87 });
    const button = Array.from(container.querySelectorAll("button"))
      .find((item) => item.textContent?.includes("测试连接"));
    expect(button).toBeInstanceOf(HTMLButtonElement);
    expect(button?.disabled).toBe(false);

    await act(async () => button?.click());

    expect(invoke).toHaveBeenCalledWith("test_deepgram_connection", { model: "nova-3" });
    expect(container.querySelector("[role='status']")?.textContent)
      .toBe("API 与模型验证成功 · nova-3 · 87 ms");
  });

  it("shows connection failures without exposing credential contents", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("Deepgram 拒绝授权"));
    const button = Array.from(container.querySelectorAll("button"))
      .find((item) => item.textContent?.includes("测试连接"));

    await act(async () => button?.click());

    expect(container.querySelector(".deepgram-test-status.error")?.textContent)
      .toContain("Deepgram 拒绝授权");
    expect(container.querySelector(".deepgram-test-status.error")?.textContent)
      .not.toContain("Authorization");
  });

  it("requires a Keychain credential before enabling the online test", async () => {
    await renderDialog(false);
    const button = Array.from(container.querySelectorAll("button"))
      .find((item) => item.textContent?.includes("先保存密钥"));

    expect(button).toBeInstanceOf(HTMLButtonElement);
    expect(button?.disabled).toBe(true);
    button?.click();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("clears a successful result when the selected model changes", async () => {
    vi.mocked(invoke).mockResolvedValue({ model: "nova-3", elapsedMs: 87 });
    const button = Array.from(container.querySelectorAll("button"))
      .find((item) => item.textContent?.includes("测试连接"));
    await act(async () => button?.click());
    expect(container.querySelector(".deepgram-test-status.success")).not.toBeNull();

    const modelSelect = container.querySelector<HTMLSelectElement>(
      "#deepgram-transcription-model",
    );
    await act(async () => {
      if (!modelSelect) return;
      modelSelect.value = "nova-2";
      modelSelect.dispatchEvent(new Event("change", { bubbles: true }));
    });

    expect(modelSelect?.value).toBe("nova-2");
    expect(container.querySelector(".deepgram-test-status")).toBeNull();
  });

  it("clears the result and disables testing after the saved key is removed", async () => {
    vi.mocked(invoke).mockResolvedValue({ model: "nova-3", elapsedMs: 87 });
    const button = Array.from(container.querySelectorAll("button"))
      .find((item) => item.textContent?.includes("测试连接"));
    await act(async () => button?.click());
    expect(container.querySelector(".deepgram-test-status.success")).not.toBeNull();

    await renderDialog(false);

    expect(container.querySelector(".deepgram-test-status")).toBeNull();
    const disabledButton = Array.from(container.querySelectorAll("button"))
      .find((item) => item.textContent?.includes("先保存密钥"));
    expect(disabledButton?.disabled).toBe(true);
  });
});
