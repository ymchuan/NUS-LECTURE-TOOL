import { invoke } from "@tauri-apps/api/core";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  BookOpen,
  Bookmark,
  BrainCircuit,
  Check,
  ChevronDownCircle,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  CircleStop,
  Clock3,
  Download,
  ExternalLink,
  FileText,
  Globe2,
  History,
  ListChecks,
  Languages,
  LoaderCircle,
  LocateFixed,
  Maximize2,
  Mic,
  Pause,
  Pencil,
  Play,
  Plus,
  RotateCcw,
  Search,
  Send,
  Settings,
  Sparkles,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DocumentPagePreview } from "./components/DocumentPagePreview";
import { RichMessage } from "./components/RichMessage";
import { TranslationDialog } from "./components/TranslationDialog";
import { useLectureSession, type StartLectureOptions } from "./hooks/useLectureSession";
import { emptyCourseSettings, settingsForCourse } from "./lib/courses";
import {
  mergeGlossaryTerms,
  recommendedGlossaryPreset,
} from "./lib/glossaryPresets";
import {
  ASR_CONFIG_VERSION,
  DEEPGRAM_ASR_MODEL,
  LOCAL_TRANSLATION_ENDPOINT,
  LOCAL_TRANSLATION_MODEL,
  migrateAsrPreferences,
  migrateTextPreferences,
  QWEN3_ASR_REALTIME_MODEL,
  QWEN_AUDIO_STREAMING_MODEL,
  summaryModelForKind,
  TEXT_CONFIG_VERSION,
  TEXT_MODEL_DEFAULTS,
  usesAlibabaSingaporeAsr,
} from "./lib/modelPreferences";
import { isNearScrollBottom } from "./lib/scroll";
import { requiredSlideConfirmations } from "./lib/slides";
import {
  isLegacyPowerPointFileName,
  isSupportedSlidesFileName,
  parseLegacyPowerPointPages,
  slidesMimeType,
} from "./lib/powerpoint";
import { buildLectureSummaryInput } from "./lib/summary";
import { translateArchivedSegment } from "./lib/translation";
import type {
  Course,
  ChatAnswer,
  ChatCitation,
  ChatMessage,
  ChatScope,
  DocumentChunk,
  DocumentSource,
  CourseSettings,
  GlossaryTerm,
  LectureBookmark,
  LectureDocument,
  LectureListItem,
  LectureSnapshot,
  SlideMatch,
  SessionStatus,
  SummaryPreferences,
  TopicSummary,
  TranscriptSegment,
} from "./types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

type CourseDraft = Pick<Course, "code" | "name" | "description"> & {
  id?: number;
};

const blankTerm = (): GlossaryTerm => ({
  english: "",
  chinese: "",
  aliases: "",
  priority: 2,
  enabled: true,
});

const defaultSummaryPreferences: SummaryPreferences = {
  asrConfigVersion: ASR_CONFIG_VERSION,
  textConfigVersion: TEXT_CONFIG_VERSION,
  autoSummaryEnabled: true,
  webSearchEnabled: true,
  transcriptionProvider: "openai",
  transcriptionModel: "gpt-live-transcribe",
  microphoneDeviceId: "",
  textProvider: "openai",
  translationProvider: "openai",
  alibabaWorkspaceId: "",
  translationModel: "gpt-5.4-nano",
  localTranslationModel: LOCAL_TRANSLATION_MODEL,
  localTranslationEndpoint: LOCAL_TRANSLATION_ENDPOINT,
  summaryModel: "gpt-5.4-mini",
  lectureSummaryModel: "gpt-5.4-mini",
};

function loadSummaryPreferences(): SummaryPreferences {
  try {
    const saved = JSON.parse(localStorage.getItem("lecture-assistant-summary-preferences") ?? "{}");
    const preferences = { ...defaultSummaryPreferences, ...saved };
    return migrateTextPreferences(migrateAsrPreferences(preferences, saved), saved);
  } catch {
    return defaultSummaryPreferences;
  }
}

function formatTime(milliseconds: number) {
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return [hours, minutes, seconds].map((value) => String(value).padStart(2, "0")).join(":");
}

function formatTimestamp(milliseconds: number) {
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function shortModelName(model: string) {
  const normalized = displayModelName(model);
  if (!normalized) return "--";
  return normalized.length > 16 ? `${normalized.slice(0, 14)}…` : normalized;
}

function displayModelName(model: string) {
  return model.trim().replace(/^translategemma(?=:|$)/, "gemma");
}

function latencyClass(milliseconds: number | null) {
  if (milliseconds === null) return "waiting";
  return milliseconds < 1_000 ? "fast" : milliseconds < 1_800 ? "moderate" : "slow";
}

const statusLabels: Record<SessionStatus, string> = {
  idle: "准备就绪",
  connecting: "正在连接",
  live: "实时听课",
  paused: "已暂停",
  recovered: "记录已恢复",
  demo: "演示课堂",
  error: "连接异常",
};

function loadLegacyCourse(): { course: CourseDraft; terms: GlossaryTerm[] } {
  try {
    const value = JSON.parse(localStorage.getItem("lecture-assistant-settings") ?? "{}") as {
      courseName?: string;
      courseContext?: string;
      keywords?: string[];
    };
    return {
      course: {
        code: "",
        name: value.courseName?.trim() || "NUS Lecture",
        description:
          value.courseContext?.trim() || "An undergraduate university lecture in Singapore.",
      },
      terms: (value.keywords ?? []).map((english) => ({
        ...blankTerm(),
        english,
      })),
    };
  } catch {
    return {
      course: {
        code: "",
        name: "NUS Lecture",
        description: "An undergraduate university lecture in Singapore.",
      },
      terms: [],
    };
  }
}

async function ensureRecommendedGlossary(
  course: Course,
  currentTerms: GlossaryTerm[],
): Promise<GlossaryTerm[]> {
  const preset = recommendedGlossaryPreset(course);
  if (!preset || !isTauri()) return currentTerms;
  const markerKey = `lecture-assistant-glossary-preset:${course.id}:${preset.id}`;
  if (currentTerms.length) {
    localStorage.setItem(markerKey, String(preset.version));
    return currentTerms;
  }
  if (localStorage.getItem(markerKey) === String(preset.version)) return currentTerms;
  const merged = mergeGlossaryTerms(currentTerms, preset.terms);
  const saved = await invoke<GlossaryTerm[]>("replace_glossary_terms", {
    courseId: course.id,
    terms: merged.terms,
  });
  localStorage.setItem(markerKey, String(preset.version));
  return saved;
}

export function CourseDialog({
  open,
  courses,
  initialCourse,
  initialTerms,
  initialSummaryPreferences,
  hasOpenAiApiKey,
  hasAlibabaApiKey,
  hasAlibabaAsrApiKey,
  hasDeepgramApiKey,
  courseLocked,
  onClose,
  onLoadCourse,
  onSave,
  onDeleteKey,
}: {
  open: boolean;
  courses: Course[];
  initialCourse: Course | null;
  initialTerms: GlossaryTerm[];
  hasOpenAiApiKey: boolean;
  hasAlibabaApiKey: boolean;
  hasAlibabaAsrApiKey: boolean;
  hasDeepgramApiKey: boolean;
  courseLocked: boolean;
  onClose: () => void;
  onLoadCourse: (courseId: number) => Promise<{ course: Course; terms: GlossaryTerm[] }>;
  initialSummaryPreferences: SummaryPreferences;
  onSave: (
    course: CourseDraft,
    terms: GlossaryTerm[],
    openAiApiKey: string,
    alibabaApiKey: string,
    alibabaAsrApiKey: string,
    deepgramApiKey: string,
    summaryPreferences: SummaryPreferences,
  ) => Promise<void>;
  onDeleteKey: (provider: "openai" | "alibaba" | "alibaba-asr" | "deepgram") => Promise<void>;
}) {
  const [draft, setDraft] = useState<CourseDraft>({ code: "", name: "", description: "" });
  const [terms, setTerms] = useState<GlossaryTerm[]>([]);
  const [openAiApiKey, setOpenAiApiKey] = useState("");
  const [alibabaApiKey, setAlibabaApiKey] = useState("");
  const [alibabaAsrApiKey, setAlibabaAsrApiKey] = useState("");
  const [deepgramApiKey, setDeepgramApiKey] = useState("");
  const [summaryPreferences, setSummaryPreferences] = useState(initialSummaryPreferences);
  const [saving, setSaving] = useState(false);
  const [loadingCourse, setLoadingCourse] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [glossaryNotice, setGlossaryNotice] = useState("");
  const [termQuery, setTermQuery] = useState("");
  const [audioInputs, setAudioInputs] = useState<MediaDeviceInfo[]>([]);
  const [testingLocalTranslation, setTestingLocalTranslation] = useState(false);
  const [localTranslationStatus, setLocalTranslationStatus] = useState("");
  const localTranslationTestRequestRef = useRef(0);
  const [testingDeepgram, setTestingDeepgram] = useState(false);
  const [deepgramTestStatus, setDeepgramTestStatus] = useState<{
    kind: "success" | "error";
    text: string;
  } | null>(null);
  const deepgramTestRequestRef = useRef(0);

  useEffect(() => {
    if (!open) return;
    setDraft(
      initialCourse
        ? {
            id: initialCourse.id,
            code: initialCourse.code,
            name: initialCourse.name,
            description: initialCourse.description,
          }
        : { code: "", name: "", description: "" },
    );
    setTerms(initialTerms);
    setOpenAiApiKey("");
    setAlibabaApiKey("");
    setAlibabaAsrApiKey("");
    setDeepgramApiKey("");
    setSummaryPreferences(initialSummaryPreferences);
    setFormError(null);
    setGlossaryNotice("");
    setTermQuery("");
    localTranslationTestRequestRef.current += 1;
    setTestingLocalTranslation(false);
    setLocalTranslationStatus("");
    deepgramTestRequestRef.current += 1;
    setTestingDeepgram(false);
    setDeepgramTestStatus(null);
    if (navigator.mediaDevices?.enumerateDevices) {
      void navigator.mediaDevices.enumerateDevices()
        .then((devices) => setAudioInputs(devices.filter((device) => device.kind === "audioinput")))
        .catch(() => setAudioInputs([]));
    }
  }, [initialCourse, initialSummaryPreferences, initialTerms, open]);

  useEffect(() => {
    if (hasDeepgramApiKey) return;
    deepgramTestRequestRef.current += 1;
    setTestingDeepgram(false);
    setDeepgramTestStatus(null);
  }, [hasDeepgramApiKey]);

  if (!open) return null;

  const normalizedTermQuery = termQuery.trim().toLocaleLowerCase();
  const visibleTerms = terms
    .map((item, index) => ({ item, index }))
    .filter(({ item }) => !normalizedTermQuery || [
      item.english,
      item.chinese,
      item.aliases,
    ].some((value) => value.toLocaleLowerCase().includes(normalizedTermQuery)));

  const chooseCourse = async (courseId: number) => {
    if (courseLocked && courseId !== initialCourse?.id) return;
    setLoadingCourse(true);
    setFormError(null);
    try {
      const selected = await onLoadCourse(courseId);
      setDraft({
        id: selected.course.id,
        code: selected.course.code,
        name: selected.course.name,
        description: selected.course.description,
      });
      setTerms(selected.terms);
      setGlossaryNotice("");
      setTermQuery("");
    } catch (reason) {
      setFormError(String(reason));
    } finally {
      setLoadingCourse(false);
    }
  };

  const updateTerm = (index: number, patch: Partial<GlossaryTerm>) => {
    setTerms((current) =>
      current.map((term, currentIndex) =>
        currentIndex === index ? { ...term, ...patch } : term,
      ),
    );
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setSaving(true);
    setFormError(null);
    try {
      await onSave(
        draft,
        terms,
        openAiApiKey,
        alibabaApiKey,
        alibabaAsrApiKey,
        deepgramApiKey,
        summaryPreferences,
      );
      onClose();
    } catch (reason) {
      setFormError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  const testLocalTranslation = async () => {
    if (!isTauri()) {
      setLocalTranslationStatus("请在桌面应用中测试本地模型");
      return;
    }
    const requestId = localTranslationTestRequestRef.current + 1;
    localTranslationTestRequestRef.current = requestId;
    const endpoint = summaryPreferences.localTranslationEndpoint;
    const model = summaryPreferences.localTranslationModel;
    setTestingLocalTranslation(true);
    setLocalTranslationStatus("");
    try {
      const result = await invoke<{ text: string; elapsedMs: number }>("test_local_translation", {
        endpoint,
        model,
      });
      if (localTranslationTestRequestRef.current !== requestId) return;
      setLocalTranslationStatus(`连接成功 · ${result.elapsedMs} ms · ${result.text}`);
    } catch (reason) {
      if (localTranslationTestRequestRef.current !== requestId) return;
      setLocalTranslationStatus(`连接失败 · ${String(reason)}`);
    } finally {
      if (localTranslationTestRequestRef.current === requestId) {
        setTestingLocalTranslation(false);
      }
    }
  };

  const updateTranslationPreferences = (patch: Partial<SummaryPreferences>) => {
    localTranslationTestRequestRef.current += 1;
    setTestingLocalTranslation(false);
    setLocalTranslationStatus("");
    setSummaryPreferences((current) => ({ ...current, ...patch }));
  };

  const updateDeepgramPreferences = (patch: Partial<SummaryPreferences>) => {
    deepgramTestRequestRef.current += 1;
    setTestingDeepgram(false);
    setDeepgramTestStatus(null);
    setSummaryPreferences((current) => ({ ...current, ...patch }));
  };

  const testDeepgramConnection = async () => {
    if (!isTauri()) {
      setDeepgramTestStatus({ kind: "error", text: "请在桌面应用中测试 Deepgram" });
      return;
    }
    if (!hasDeepgramApiKey) {
      setDeepgramTestStatus({
        kind: "error",
        text: "请先保存设置，将 Deepgram API Key 存入 Keychain",
      });
      return;
    }
    const requestId = deepgramTestRequestRef.current + 1;
    deepgramTestRequestRef.current = requestId;
    const model = summaryPreferences.transcriptionModel;
    setTestingDeepgram(true);
    setDeepgramTestStatus(null);
    try {
      const result = await invoke<{ model: string; elapsedMs: number }>(
        "test_deepgram_connection",
        { model },
      );
      if (deepgramTestRequestRef.current !== requestId) return;
      setDeepgramTestStatus({
        kind: "success",
        text: `API 与模型验证成功 · ${result.model} · ${result.elapsedMs} ms`,
      });
    } catch (reason) {
      if (deepgramTestRequestRef.current !== requestId) return;
      setDeepgramTestStatus({ kind: "error", text: `测试失败 · ${String(reason)}` });
    } finally {
      if (deepgramTestRequestRef.current === requestId) {
        setTestingDeepgram(false);
      }
    }
  };

  const importRecommendedTerms = () => {
    const preset = recommendedGlossaryPreset({
      code: draft.code,
      name: draft.name,
      description: draft.description,
    });
    if (!preset) return;
    const merged = mergeGlossaryTerms(terms, preset.terms);
    setTerms(merged.terms);
    setGlossaryNotice(
      merged.added ? `已加入 ${merged.added} 个推荐术语` : "推荐术语已经全部存在",
    );
  };

  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        className="settings-dialog course-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="dialog-header">
          <div>
            <p className="eyebrow">课堂设置</p>
            <h2 id="settings-title">课程与术语</h2>
          </div>
          <button className="icon-button" type="button" onClick={onClose} title="关闭">
            <X size={19} />
          </button>
        </header>

        <form className="course-editor" onSubmit={submit}>
          <aside className="course-list" aria-label="课程列表">
            <div className="course-list-header">
              <strong>我的课程</strong>
              <button
                className="icon-button"
                type="button"
                title="新建课程"
                disabled={courseLocked}
                onClick={() => {
                  setDraft({ code: "", name: "", description: "" });
                  setTerms([]);
                }}
              >
                <Plus size={17} />
              </button>
            </div>
            <div className="course-list-scroll">
              {courses.map((course) => (
                <button
                  className={`course-list-item ${draft.id === course.id ? "selected" : ""}`}
                  type="button"
                  key={course.id}
                  disabled={loadingCourse || (courseLocked && course.id !== initialCourse?.id)}
                  onClick={() => void chooseCourse(course.id)}
                >
                  <span>{course.code || "NUS"}</span>
                  <strong>{course.name}</strong>
                  {draft.id === course.id && <Check size={15} />}
                </button>
              ))}
            </div>
          </aside>

          <div className="course-editor-main">
            <div className="form-section course-fields">
              <div className="field-pair">
                <div>
                  <label htmlFor="course-code">课程代码</label>
                  <input
                    id="course-code"
                    value={draft.code}
                    placeholder="例如：EC1101E"
                    onChange={(event) => setDraft({ ...draft, code: event.target.value })}
                  />
                </div>
                <div>
                  <label htmlFor="course-name">课程名称</label>
                  <input
                    id="course-name"
                    value={draft.name}
                    placeholder="课程名称"
                    onChange={(event) => setDraft({ ...draft, name: event.target.value })}
                    required
                  />
                </div>
              </div>

              <label htmlFor="course-context">Syllabus 与课程说明</label>
              <textarea
                id="course-context"
                rows={6}
                value={draft.description}
                placeholder="直接粘贴课程 Syllabus、学习目标、考核方式和课程主题"
                onChange={(event) => setDraft({ ...draft, description: event.target.value })}
              />
            </div>

            <div className="form-section glossary-section">
              <div className="section-title-row">
                <div>
                  <strong>专业词表与 ASR 热词</strong>
                  <span>
                    {terms.filter((term) => term.enabled && term.english.trim()).length} 个已启用
                    {glossaryNotice ? ` · ${glossaryNotice}` : ""}
                  </span>
                </div>
                <div className="section-actions">
                  <label className="glossary-search" title="搜索术语">
                    <Search size={14} />
                    <input
                      value={termQuery}
                      placeholder="搜索术语"
                      aria-label="搜索专业术语"
                      onChange={(event) => setTermQuery(event.target.value)}
                    />
                  </label>
                  {recommendedGlossaryPreset({
                    code: draft.code,
                    name: draft.name,
                    description: draft.description,
                  }) && (
                    <button
                      className="secondary-button compact-button"
                      type="button"
                      onClick={importRecommendedTerms}
                    >
                      <Sparkles size={15} />
                      补充推荐词表
                    </button>
                  )}
                  <button
                    className="secondary-button compact-button"
                    type="button"
                    onClick={() => {
                      setTermQuery("");
                      setTerms((current) => [...current, blankTerm()]);
                    }}
                  >
                    <Plus size={15} />
                    添加术语
                  </button>
                </div>
              </div>

              <div className="glossary-table">
                {visibleTerms.length ? (
                  visibleTerms.map(({ item: term, index }) => (
                    <div className={`term-row ${term.enabled ? "" : "disabled-term"}`} key={term.id ?? `new-${index}`}>
                      <input
                        className="term-toggle"
                        type="checkbox"
                        checked={term.enabled}
                        aria-label={`启用 ${term.english || "新术语"}`}
                        onChange={(event) => updateTerm(index, { enabled: event.target.checked })}
                      />
                      <input
                        value={term.english}
                        placeholder="English term"
                        aria-label="英文术语"
                        onChange={(event) => updateTerm(index, { english: event.target.value })}
                      />
                      <input
                        value={term.chinese}
                        placeholder="标准中文译法"
                        aria-label="中文译法"
                        onChange={(event) => updateTerm(index, { chinese: event.target.value })}
                      />
                      <select
                        value={term.priority}
                        aria-label="术语优先级"
                        onChange={(event) => updateTerm(index, { priority: Number(event.target.value) })}
                      >
                        <option value={3}>高</option>
                        <option value={2}>中</option>
                        <option value={1}>低</option>
                      </select>
                      <button
                        className="icon-button danger-quiet"
                        type="button"
                        title="删除术语"
                        onClick={() => setTerms((current) => current.filter((_, itemIndex) => itemIndex !== index))}
                      >
                        <Trash2 size={16} />
                      </button>
                      <input
                        className="term-aliases"
                        value={term.aliases}
                        placeholder="别名或缩写，用逗号分隔"
                        aria-label="术语别名"
                        onChange={(event) => updateTerm(index, { aliases: event.target.value })}
                      />
                    </div>
                  ))
                ) : (
                  <div className="glossary-empty">
                    {terms.length ? "没有匹配的术语" : "尚未添加术语"}
                  </div>
                )}
              </div>
            </div>

            <div className="form-section summary-preferences">
              <div className="section-title-row">
                <div>
                  <strong>实时英文转写</strong>
                  <span>{summaryPreferences.transcriptionModel}</span>
                </div>
                {summaryPreferences.transcriptionProvider === "deepgram" && (
                  <div className="section-actions">
                    <button
                      className="secondary-button compact-button"
                      type="button"
                      disabled={!hasDeepgramApiKey || testingDeepgram}
                      title={hasDeepgramApiKey
                        ? "向 Deepgram 发送约 0.5 秒静音，验证网络、密钥和模型权限"
                        : "请先保存设置，将 Deepgram API Key 存入 Keychain"}
                      onClick={() => void testDeepgramConnection()}
                    >
                      <Play size={13} />
                      {testingDeepgram
                        ? "测试中"
                        : hasDeepgramApiKey
                          ? "测试连接"
                          : "先保存密钥"}
                    </button>
                  </div>
                )}
              </div>
              <label className="model-select-row" htmlFor="microphone-device">
                <span>
                  <strong>课堂麦克风</strong>
                  <small>外接定向麦克风通常能明显改善远距离识别</small>
                </span>
                <select
                  id="microphone-device"
                  value={summaryPreferences.microphoneDeviceId}
                  onChange={(event) => setSummaryPreferences({
                    ...summaryPreferences,
                    microphoneDeviceId: event.target.value,
                  })}
                >
                  <option value="">系统默认麦克风</option>
                  {summaryPreferences.microphoneDeviceId
                    && !audioInputs.some((device) => device.deviceId === summaryPreferences.microphoneDeviceId) && (
                    <option value={summaryPreferences.microphoneDeviceId}>上次选择的麦克风</option>
                  )}
                  {audioInputs
                    .filter((device) => device.deviceId && device.deviceId !== "default")
                    .map((device, index) => (
                      <option key={device.deviceId} value={device.deviceId}>
                        {device.label || `麦克风 ${index + 1}`}
                      </option>
                    ))}
                </select>
              </label>
              <div className="model-provider-control asr-provider-control" aria-label="实时转写供应商">
                <button
                  className={summaryPreferences.transcriptionProvider === "openai" ? "active" : ""}
                  type="button"
                  onClick={() => setSummaryPreferences({
                    ...summaryPreferences,
                    transcriptionProvider: "openai",
                    transcriptionModel: "gpt-live-transcribe",
                  })}
                >
                  <strong>OpenAI</strong>
                  <span>低延迟 WebRTC</span>
                </button>
                <button
                  className={summaryPreferences.transcriptionProvider === "alibaba" ? "active" : ""}
                  type="button"
                  onClick={() => setSummaryPreferences({
                    ...summaryPreferences,
                    transcriptionProvider: "alibaba",
                    transcriptionModel: QWEN3_ASR_REALTIME_MODEL,
                  })}
                >
                  <strong>阿里云</strong>
                  <span>新加坡低延迟</span>
                </button>
                <button
                  className={summaryPreferences.transcriptionProvider === "deepgram" ? "active" : ""}
                  type="button"
                  onClick={() => updateDeepgramPreferences({
                    transcriptionProvider: "deepgram",
                    transcriptionModel: DEEPGRAM_ASR_MODEL,
                  })}
                >
                  <strong>Deepgram</strong>
                  <span>Nova-3 / Nova-2</span>
                </button>
              </div>
              {summaryPreferences.transcriptionProvider === "alibaba" && (
                <label className="model-select-row" htmlFor="transcription-model">
                  <span>
                    <strong>转写模型</strong>
                    <small>
                      {summaryPreferences.transcriptionModel === QWEN3_ASR_REALTIME_MODEL
                        ? "新加坡 · 固定英语 · 强口音"
                        : summaryPreferences.transcriptionModel === QWEN_AUDIO_STREAMING_MODEL
                          ? "新加坡 · 英语提示 · 热词"
                          : "北京 · 低成本"}
                    </small>
                  </span>
                  <select
                    id="transcription-model"
                    value={summaryPreferences.transcriptionModel}
                    onChange={(event) => setSummaryPreferences({
                      ...summaryPreferences,
                      transcriptionModel: event.target.value,
                    })}
                  >
                    <option value={QWEN3_ASR_REALTIME_MODEL}>Qwen3 ASR · 固定英语</option>
                    <option value={QWEN_AUDIO_STREAMING_MODEL}>Qwen Audio 3 · 英语提示</option>
                    <option value="paraformer-realtime-v2">Paraformer Realtime v2</option>
                  </select>
                </label>
              )}
              {summaryPreferences.transcriptionProvider === "deepgram" && (
                <label className="model-select-row" htmlFor="deepgram-transcription-model">
                  <span>
                    <strong>转写模型</strong>
                    <small>英语 · 在线测试会发送约 0.5 秒静音</small>
                  </span>
                  <select
                    id="deepgram-transcription-model"
                    value={summaryPreferences.transcriptionModel}
                    onChange={(event) => updateDeepgramPreferences({
                      transcriptionModel: event.target.value,
                    })}
                  >
                    <option value={DEEPGRAM_ASR_MODEL}>Nova-3</option>
                    <option value="nova-2">Nova-2</option>
                  </select>
                </label>
              )}
              {summaryPreferences.transcriptionProvider === "deepgram"
                && deepgramTestStatus && (
                <div
                  className={`deepgram-test-status ${deepgramTestStatus.kind}`}
                  role="status"
                  aria-live="polite"
                >
                  {deepgramTestStatus.text}
                </div>
              )}
            </div>

            <div className="form-section summary-preferences">
              <div className="section-title-row">
                <div>
                  <strong>实时翻译</strong>
                  <span>
                    {summaryPreferences.translationProvider === "local"
                      ? summaryPreferences.localTranslationModel
                      : summaryPreferences.translationModel}
                  </span>
                </div>
                {summaryPreferences.translationProvider === "local" && (
                  <div className="section-actions">
                    <button
                      className="secondary-button compact-button"
                      type="button"
                      disabled={testingLocalTranslation}
                      onClick={() => void testLocalTranslation()}
                    >
                      <Play size={13} />
                      {testingLocalTranslation ? "测试中" : "测试模型"}
                    </button>
                  </div>
                )}
              </div>
              <div className="model-provider-control translation-provider-control" aria-label="实时翻译供应商">
                <button
                  className={summaryPreferences.translationProvider === "openai" ? "active" : ""}
                  type="button"
                  onClick={() => updateTranslationPreferences({
                    translationProvider: "openai",
                    translationModel: TEXT_MODEL_DEFAULTS.openai.translationModel,
                  })}
                >
                  <strong>OpenAI</strong>
                  <span>云端</span>
                </button>
                <button
                  className={summaryPreferences.translationProvider === "alibaba" ? "active" : ""}
                  type="button"
                  onClick={() => updateTranslationPreferences({
                    translationProvider: "alibaba",
                    translationModel: TEXT_MODEL_DEFAULTS.alibaba.translationModel,
                  })}
                >
                  <strong>阿里云百炼</strong>
                  <span>云端</span>
                </button>
                <button
                  className={summaryPreferences.translationProvider === "local" ? "active" : ""}
                  type="button"
                  onClick={() => updateTranslationPreferences({
                    translationProvider: "local",
                  })}
                >
                  <strong>本地模型</strong>
                  <span>离线</span>
                </button>
              </div>
              {summaryPreferences.translationProvider === "local" ? (
                <>
                  <label className="model-select-row" htmlFor="local-translation-model">
                    <span>
                      <strong>本地模型</strong>
                      <small>名称需与本地服务一致</small>
                    </span>
                    <input
                      id="local-translation-model"
                      list="local-translation-model-options"
                      value={summaryPreferences.localTranslationModel}
                      onChange={(event) => updateTranslationPreferences({
                        localTranslationModel: event.target.value,
                      })}
                    />
                    <datalist id="local-translation-model-options">
                      <option value="translategemma:4b" />
                      <option value="qwen3:4b" />
                      <option value="qwen3:4b-instruct-2507-q4_K_M" />
                      <option value="qwen3.5:4b" />
                    </datalist>
                  </label>
                  <label className="model-select-row" htmlFor="local-translation-endpoint">
                    <span>
                      <strong>本地服务地址</strong>
                      <small>仅允许本机回环地址</small>
                    </span>
                    <input
                      id="local-translation-endpoint"
                      type="url"
                      spellCheck={false}
                      value={summaryPreferences.localTranslationEndpoint}
                      onChange={(event) => updateTranslationPreferences({
                        localTranslationEndpoint: event.target.value.trim(),
                      })}
                    />
                  </label>
                  {!!localTranslationStatus && (
                    <div className="local-model-status" role="status">{localTranslationStatus}</div>
                  )}
                </>
              ) : (
                <label className="model-select-row" htmlFor="translation-model">
                  <span>
                    <strong>翻译模型</strong>
                    <small>低延迟优先</small>
                  </span>
                  <select
                    id="translation-model"
                    value={summaryPreferences.translationModel}
                    onChange={(event) => setSummaryPreferences({
                      ...summaryPreferences,
                      translationModel: event.target.value,
                    })}
                  >
                    {summaryPreferences.translationProvider === "openai" ? (
                      <>
                        <option value="gpt-5.4-nano">gpt-5.4-nano</option>
                        <option value="gpt-5.4-mini">gpt-5.4-mini</option>
                      </>
                    ) : (
                      <>
                        <option value="qwen-mt-lite">qwen-mt-lite · 最低延迟</option>
                        <option value="qwen-mt-flash">qwen-mt-flash · 术语优先</option>
                        <option value="qwen3.7-flash">qwen3.7-flash · 高并发</option>
                        <option value="qwen3.5-flash">qwen3.5-flash · 旧版</option>
                      </>
                    )}
                  </select>
                </label>
              )}
            </div>

            <div className="form-section summary-preferences">
              <div className="section-title-row">
                <div>
                  <strong>总结与问答</strong>
                  <span>{summaryPreferences.summaryModel}</span>
                </div>
              </div>
              <div className="model-provider-control" aria-label="总结与问答供应商">
                <button
                  className={summaryPreferences.textProvider === "openai" ? "active" : ""}
                  type="button"
                  onClick={() => setSummaryPreferences({
                    ...summaryPreferences,
                    textProvider: "openai",
                    summaryModel: TEXT_MODEL_DEFAULTS.openai.summaryModel,
                    lectureSummaryModel: TEXT_MODEL_DEFAULTS.openai.lectureSummaryModel,
                  })}
                >
                  <strong>OpenAI</strong>
                  <span>稳定默认</span>
                </button>
                <button
                  className={summaryPreferences.textProvider === "alibaba" ? "active" : ""}
                  type="button"
                  onClick={() => setSummaryPreferences({
                    ...summaryPreferences,
                    textProvider: "alibaba",
                    summaryModel: TEXT_MODEL_DEFAULTS.alibaba.summaryModel,
                    lectureSummaryModel: TEXT_MODEL_DEFAULTS.alibaba.lectureSummaryModel,
                  })}
                >
                  <strong>阿里云百炼</strong>
                  <span>新加坡 · 分层优化</span>
                </button>
              </div>
              <label className="model-select-row" htmlFor="summary-model">
                <span>
                  <strong>阶段总结</strong>
                  <small>速度与质量平衡</small>
                </span>
                <select
                  id="summary-model"
                  value={summaryPreferences.summaryModel}
                  onChange={(event) => setSummaryPreferences({
                    ...summaryPreferences,
                    summaryModel: event.target.value,
                  })}
                >
                  {summaryPreferences.textProvider === "openai" ? (
                    <>
                      <option value="gpt-5.4-mini">gpt-5.4-mini</option>
                      <option value="gpt-5.4-nano">gpt-5.4-nano</option>
                    </>
                  ) : (
                    <>
                      <option value="qwen3.7-flash">qwen3.7-flash · 推荐</option>
                      <option value="qwen3.7-plus">qwen3.7-plus</option>
                    </>
                  )}
                </select>
              </label>
              <label className="model-select-row" htmlFor="lecture-summary-model">
                <span>
                  <strong>整课总结</strong>
                  <small>质量优先</small>
                </span>
                <select
                  id="lecture-summary-model"
                  value={summaryPreferences.lectureSummaryModel}
                  onChange={(event) => setSummaryPreferences({
                    ...summaryPreferences,
                    lectureSummaryModel: event.target.value,
                  })}
                >
                  {summaryPreferences.textProvider === "openai" ? (
                    <>
                      <option value="gpt-5.4-mini">gpt-5.4-mini · 推荐</option>
                      <option value="gpt-5.4-nano">gpt-5.4-nano</option>
                    </>
                  ) : (
                    <>
                      <option value="qwen3.7-plus">qwen3.7-plus · 推荐</option>
                      <option value="qwen3.7-flash">qwen3.7-flash · 经济</option>
                      <option value="qwen3.8-max">qwen3.8-max · 深度</option>
                    </>
                  )}
                </select>
              </label>
              <label className="setting-toggle">
                <span>
                  <strong>自动识别主题边界</strong>
                  <small>在转场或内容达到时间护栏时生成阶段总结</small>
                </span>
                <input
                  type="checkbox"
                  checked={summaryPreferences.autoSummaryEnabled}
                  onChange={(event) =>
                    setSummaryPreferences({
                      ...summaryPreferences,
                      autoSummaryEnabled: event.target.checked,
                    })
                  }
                />
              </label>
              <label className="setting-toggle">
                <span>
                  <strong>联网查找相关拓展</strong>
                  <small>补充资料并显示可点击来源，搜索工具另行计费</small>
                </span>
                <input
                  type="checkbox"
                  checked={summaryPreferences.webSearchEnabled}
                  onChange={(event) =>
                    setSummaryPreferences({
                      ...summaryPreferences,
                      webSearchEnabled: event.target.checked,
                    })
                  }
                />
              </label>
            </div>

            {(summaryPreferences.transcriptionProvider === "openai"
              || summaryPreferences.translationProvider === "openai"
              || summaryPreferences.textProvider === "openai") && (
              <div className="form-section api-section">
              <div className="label-row">
                <label htmlFor="openai-api-key">OpenAI API Key</label>
                <span className={`key-state ${hasOpenAiApiKey ? "saved" : "missing"}`}>
                  {hasOpenAiApiKey ? "已保存至 Keychain" : "未保存"}
                </span>
              </div>
              <div className="key-input-row">
                <input
                  id="openai-api-key"
                  type="password"
                  autoComplete="off"
                  value={openAiApiKey}
                  placeholder={hasOpenAiApiKey ? "输入新密钥可替换" : "sk-..."}
                  onChange={(event) => setOpenAiApiKey(event.target.value)}
                />
                {hasOpenAiApiKey && (
                  <button
                    className="icon-button danger-quiet"
                    type="button"
                    title="删除 OpenAI API Key"
                    onClick={() => void onDeleteKey("openai")}
                  >
                    <Trash2 size={18} />
                  </button>
                )}
              </div>
              </div>
            )}

            {summaryPreferences.transcriptionProvider === "deepgram" && (
              <div className="form-section api-section">
                <div className="label-row">
                  <label htmlFor="deepgram-api-key">Deepgram API Key · 实时转写</label>
                  <span className={`key-state ${hasDeepgramApiKey ? "saved" : "missing"}`}>
                    {hasDeepgramApiKey ? "已保存至 Keychain" : "未保存"}
                  </span>
                </div>
                <div className="key-input-row">
                  <input
                    id="deepgram-api-key"
                    type="password"
                    autoComplete="off"
                    value={deepgramApiKey}
                    placeholder={hasDeepgramApiKey ? "输入新密钥可替换" : "Deepgram API Key"}
                    onChange={(event) => setDeepgramApiKey(event.target.value)}
                  />
                  {hasDeepgramApiKey && (
                    <button
                      className="icon-button danger-quiet"
                      type="button"
                      title="删除 Deepgram API Key"
                      onClick={() => void onDeleteKey("deepgram")
                        .catch((reason) => setFormError(String(reason)))}
                    >
                      <Trash2 size={18} />
                    </button>
                  )}
                </div>
              </div>
            )}

            {summaryPreferences.transcriptionProvider === "alibaba"
              && summaryPreferences.transcriptionModel === "paraformer-realtime-v2" && (
              <div className="form-section api-section">
                <div className="label-row">
                  <label htmlFor="alibaba-asr-api-key">阿里云北京 API Key · 实时转写</label>
                  <span className={`key-state ${hasAlibabaAsrApiKey ? "saved" : "missing"}`}>
                    {hasAlibabaAsrApiKey ? "已保存至 Keychain" : "未保存"}
                  </span>
                </div>
                <div className="key-input-row">
                  <input
                    id="alibaba-asr-api-key"
                    type="password"
                    autoComplete="off"
                    value={alibabaAsrApiKey}
                    placeholder={hasAlibabaAsrApiKey ? "输入新密钥可替换" : "sk-..."}
                    onChange={(event) => setAlibabaAsrApiKey(event.target.value)}
                  />
                  {hasAlibabaAsrApiKey && (
                    <button
                      className="icon-button danger-quiet"
                      type="button"
                      title="删除阿里云实时转写 API Key"
                      onClick={() => void onDeleteKey("alibaba-asr")}
                    >
                      <Trash2 size={18} />
                    </button>
                  )}
                </div>
              </div>
            )}

            {(summaryPreferences.textProvider === "alibaba"
              || summaryPreferences.translationProvider === "alibaba"
              || (summaryPreferences.transcriptionProvider === "alibaba"
                && usesAlibabaSingaporeAsr(summaryPreferences.transcriptionModel))) && (
              <div className="form-section api-section">
                <div className="label-row">
                  <label htmlFor="alibaba-workspace-id">阿里云业务空间 ID</label>
                  <span className="key-state saved">推荐专属线路</span>
                </div>
                <input
                  id="alibaba-workspace-id"
                  type="text"
                  autoComplete="off"
                  spellCheck={false}
                  pattern="(ws|llm)-[a-z0-9-]+"
                  maxLength={80}
                  title="请输入类似 ws-xxxxxxxx 或 llm-xxxxxxxx 的业务空间 ID"
                  value={summaryPreferences.alibabaWorkspaceId}
                  placeholder="ws-xxxxxxxx"
                  onChange={(event) => setSummaryPreferences({
                    ...summaryPreferences,
                    alibabaWorkspaceId: event.target.value.trim(),
                  })}
                />
                <div className="label-row">
                  <label htmlFor="alibaba-api-key">
                    阿里云新加坡 API Key
                    {summaryPreferences.textProvider === "alibaba"
                      && summaryPreferences.translationProvider === "alibaba"
                      && summaryPreferences.transcriptionProvider === "alibaba"
                      && usesAlibabaSingaporeAsr(summaryPreferences.transcriptionModel)
                      ? " · 转写、翻译与总结"
                      : summaryPreferences.textProvider === "alibaba"
                        && summaryPreferences.translationProvider === "alibaba"
                        ? " · 翻译与总结"
                        : summaryPreferences.textProvider === "alibaba"
                          ? " · 总结与问答"
                          : summaryPreferences.translationProvider === "alibaba"
                            ? " · 实时翻译"
                        : " · 实时转写"}
                  </label>
                  <span className={`key-state ${hasAlibabaApiKey ? "saved" : "missing"}`}>
                    {hasAlibabaApiKey ? "已保存至 Keychain" : "未保存"}
                  </span>
                </div>
                <div className="key-input-row">
                  <input
                    id="alibaba-api-key"
                    type="password"
                    autoComplete="off"
                    value={alibabaApiKey}
                    placeholder={hasAlibabaApiKey ? "输入新密钥可替换" : "sk-..."}
                    onChange={(event) => setAlibabaApiKey(event.target.value)}
                  />
                  {hasAlibabaApiKey && (
                    <button
                      className="icon-button danger-quiet"
                      type="button"
                      title="删除阿里云百炼 API Key"
                      onClick={() => void onDeleteKey("alibaba")}
                    >
                      <Trash2 size={18} />
                    </button>
                  )}
                </div>
              </div>
            )}

            {formError && <p className="form-error">{formError}</p>}

            <footer className="dialog-actions">
              <button className="secondary-button" type="button" onClick={onClose}>
                取消
              </button>
              <button className="primary-button" type="submit" disabled={saving}>
                {saving ? "保存中" : draft.id !== undefined ? "保存课程" : "创建课程"}
              </button>
            </footer>
          </div>
        </form>
      </section>
    </div>
  );
}

function SummaryDetailDialog({
  summary,
  onClose,
  onRegenerate,
  regenerating = false,
}: {
  summary: TopicSummary | null;
  onClose: () => void;
  onRegenerate?: () => void;
  regenerating?: boolean;
}) {
  const [tab, setTab] = useState<"knowledge" | "mindmap" | "explore">("knowledge");

  useEffect(() => {
    if (summary) setTab("knowledge");
  }, [summary]);

  if (!summary) return null;

  const isLectureSummary = summary.kind === "lecture";
  const summaryStats = [
    `${summary.points.length} 条主线`,
    `${summary.knowledgePoints?.length ?? 0} 个知识点`,
    `${summary.definitions?.length ?? 0} 个术语`,
    `${summary.examples?.length ?? 0} 个例子`,
  ];
  const insightSourceUrls = new Set(
    summary.webInsights?.flatMap((insight) => insight.sources?.map((source) => source.url) ?? []) ?? [],
  );
  const otherSources = (summary.sources ?? []).filter((source) => !insightSourceUrls.has(source.url));
  const visibleTopLevelSources = insightSourceUrls.size > 0 ? otherSources : (summary.sources ?? []);
  const hasExploreContent = !!summary.webInsights?.length || visibleTopLevelSources.length > 0;

  const openSource = async (url: string) => {
    if (isTauri()) await openUrl(url);
    else window.open(url, "_blank", "noopener,noreferrer");
  };

  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        className="summary-detail-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="summary-detail-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="dialog-header summary-detail-header">
          <div>
            <p className="eyebrow">
              {isLectureSummary ? "整课复习" : "本段回顾"}{" · "}
              {formatTimestamp(summary.startMs)}
              {summary.endMs !== undefined ? ` - ${formatTimestamp(summary.endMs)}` : ""}
            </p>
            <h2 id="summary-detail-title">{summary.title}</h2>
          </div>
          <div className="summary-origin">
            <span>{isLectureSummary ? "整课复习资料" : "阶段总结"}</span>
            {summary.webEnriched && <span>联网拓展</span>}
            {summary.model && summary.model !== "demo" && <span>{summary.model}</span>}
            {isLectureSummary && onRegenerate && (
              <button
                className="summary-regenerate-button"
                type="button"
                disabled={regenerating}
                onClick={onRegenerate}
              >
                {regenerating ? <LoaderCircle className="spin" size={14} /> : <RotateCcw size={14} />}
                {regenerating ? "重新生成中" : "重新生成"}
              </button>
            )}
            <button className="icon-button" type="button" onClick={onClose} title="关闭">
              <X size={19} />
            </button>
          </div>
        </header>

        <nav className="summary-tabs" aria-label="总结视图">
          <button className={tab === "knowledge" ? "active" : ""} type="button" onClick={() => setTab("knowledge")}>
            {isLectureSummary ? "完整复习" : "本段回顾"}
          </button>
          <button className={tab === "mindmap" ? "active" : ""} type="button" onClick={() => setTab("mindmap")}>
            思维导图
          </button>
          <button className={tab === "explore" ? "active" : ""} type="button" onClick={() => setTab("explore")}>
            相关拓展
            {!!summary.sources?.length && <span>{summary.sources.length}</span>}
          </button>
        </nav>

        <div className="summary-detail-content">
          {tab === "knowledge" && (
            <div className="knowledge-view">
              <section className="summary-overview">
                <h3>{isLectureSummary ? "整课路线图" : "刚才讲了什么"}</h3>
                <p>{summary.overview || summary.points.join("；")}</p>
                {isLectureSummary && (
                  <div className="summary-coverage" aria-label="整课总结覆盖范围">
                    {summaryStats.map((item) => <span key={item}>{item}</span>)}
                  </div>
                )}
              </section>

              {isLectureSummary && !!summary.points.length && (
                <section className="lecture-roadmap">
                  <h3>整课主线</h3>
                  <ol>
                    {summary.points.map((point, index) => (
                      <li key={`${summary.id}-roadmap-${index}`}>{point}</li>
                    ))}
                  </ol>
                </section>
              )}

              {!!summary.knowledgePoints?.length && (
                <section>
                  <h3>{isLectureSummary ? "完整知识体系" : "重点解释"}</h3>
                  <div className="knowledge-list">
                    {summary.knowledgePoints.map((point, index) => (
                      <article key={`${summary.id}-knowledge-${index}`}>
                        <header>
                          <strong>{point.title}</strong>
                          <span className={point.importance === "core" ? "core" : "supporting"}>
                            {point.importance === "core" ? "核心" : "补充"}
                          </span>
                        </header>
                        <p>{point.explanation}</p>
                        {point.lecturerEvidence && <small>课堂依据：{point.lecturerEvidence}</small>}
                      </article>
                    ))}
                  </div>
                </section>
              )}

              <div className="knowledge-columns">
                {!!summary.definitions?.length && (
                  <section>
                    <h3>定义与术语</h3>
                    <dl>
                      {summary.definitions.map((item, index) => (
                        <div key={`${summary.id}-definition-${index}`}>
                          <dt>{item.term}</dt>
                          <dd>{item.definition}</dd>
                        </div>
                      ))}
                    </dl>
                  </section>
                )}
                {!!summary.examples?.length && (
                  <section>
                    <h3>例子与类比</h3>
                    <ul>{summary.examples.map((item, index) => <li key={`${summary.id}-example-${index}`}>{item}</li>)}</ul>
                  </section>
                )}
                {!!summary.examTips?.length && (
                  <section>
                    <h3>考试与作业提示</h3>
                    <ul>{summary.examTips.map((item, index) => <li key={`${summary.id}-tip-${index}`}>{item}</li>)}</ul>
                  </section>
                )}
                {!!summary.questions?.length && (
                  <section>
                    <h3>复习问题</h3>
                    <ol>{summary.questions.map((item, index) => <li key={`${summary.id}-question-${index}`}>{item}</li>)}</ol>
                  </section>
                )}
              </div>
            </div>
          )}

          {tab === "mindmap" && (
            <div className="mind-map-view">
              {summary.mindMap ? (
                <>
                  <div className="mind-map-root">
                    <BrainCircuit size={20} />
                    <strong>{summary.mindMap.root}</strong>
                  </div>
                  <div className="mind-map-branches">
                    {summary.mindMap.branches.map((branch, branchIndex) => (
                      <section key={`${summary.id}-branch-${branchIndex}`}>
                        <h3>{branch.label}</h3>
                        <div>
                          {branch.children.map((leaf, leafIndex) => (
                            <article key={`${summary.id}-leaf-${branchIndex}-${leafIndex}`}>
                              <strong>{leaf.label}</strong>
                              <span>{leaf.note}</span>
                            </article>
                          ))}
                        </div>
                      </section>
                    ))}
                  </div>
                </>
              ) : (
                <div className="detail-empty">这份旧总结没有思维导图</div>
              )}
            </div>
          )}

          {tab === "explore" && (
            <div className="explore-view">
              {!!summary.webInsights?.length && (
                <section>
                  <h3>相关知识拓展</h3>
                  <div className="insight-list">
                    {summary.webInsights.map((insight, index) => (
                      <article key={`${summary.id}-insight-${index}`}>
                        <header className="insight-heading">
                          <span>{String(index + 1).padStart(2, "0")}</span>
                          <strong>{insight.title}</strong>
                        </header>
                        <p className="insight-summary">{insight.summary}</p>
                        {!!insight.explanation?.trim() && (
                          <p className="insight-explanation">{insight.explanation}</p>
                        )}
                        {!!insight.keyPoints?.length && (
                          <div className="insight-section">
                            <h4>关键结论</h4>
                            <ul>
                              {insight.keyPoints.map((point, pointIndex) => (
                                <li key={`${summary.id}-insight-${index}-point-${pointIndex}`}>{point}</li>
                              ))}
                            </ul>
                          </div>
                        )}
                        {!!insight.formulas?.length && (
                          <div className="insight-section">
                            <h4>公式与计算</h4>
                            <div className="insight-formulas">
                              {insight.formulas.map((formula, formulaIndex) => (
                                <section key={`${summary.id}-insight-${index}-formula-${formulaIndex}`}>
                                  <h5>{formula.name}</h5>
                                  <FormulaExpression value={formula.expression} />
                                  {!!formula.variables.length && (
                                    <div className="formula-detail">
                                      <strong>变量</strong>
                                      <ul>
                                        {formula.variables.map((variable, variableIndex) => (
                                          <li key={`${summary.id}-insight-${index}-formula-${formulaIndex}-variable-${variableIndex}`}>
                                            {variable}
                                          </li>
                                        ))}
                                      </ul>
                                    </div>
                                  )}
                                  {!!formula.useWhen.trim() && (
                                    <div className="formula-detail">
                                      <strong>适用场景</strong>
                                      <p>{formula.useWhen}</p>
                                    </div>
                                  )}
                                  {!!formula.steps.length && (
                                    <div className="formula-detail">
                                      <strong>使用步骤</strong>
                                      <ol>
                                        {formula.steps.map((step, stepIndex) => (
                                          <li key={`${summary.id}-insight-${index}-formula-${formulaIndex}-step-${stepIndex}`}>
                                            {step}
                                          </li>
                                        ))}
                                      </ol>
                                    </div>
                                  )}
                                  {!!formula.workedExample.trim() && (
                                    <div className="formula-example">
                                      <strong>算例</strong>
                                      <p>{formula.workedExample}</p>
                                    </div>
                                  )}
                                </section>
                              ))}
                            </div>
                          </div>
                        )}
                        {!!insight.howToUse?.length && (
                          <div className="insight-section">
                            <h4>如何应用</h4>
                            <ol>
                              {insight.howToUse.map((step, stepIndex) => (
                                <li key={`${summary.id}-insight-${index}-usage-${stepIndex}`}>{step}</li>
                              ))}
                            </ol>
                          </div>
                        )}
                        {!!insight.sources?.length && (
                          <div className="insight-sources">
                            <h4>进一步阅读</h4>
                            <div>
                              {insight.sources.map((source, sourceIndex) => (
                                <button
                                  type="button"
                                  key={`${summary.id}-insight-${index}-source-${sourceIndex}`}
                                  onClick={() => void openSource(source.url)}
                                  title={source.url}
                                >
                                  <span>{source.title}</span>
                                  <ExternalLink size={14} />
                                </button>
                              ))}
                            </div>
                          </div>
                        )}
                      </article>
                    ))}
                  </div>
                </section>
              )}
              {!!visibleTopLevelSources.length && (
                <section>
                  <h3>{insightSourceUrls.size > 0 ? "其他检索来源" : "参考来源"}</h3>
                  <div className="source-list">
                    {visibleTopLevelSources.map((source, index) => (
                      <button type="button" key={`${summary.id}-source-${index}`} onClick={() => void openSource(source.url)}>
                        <span>
                          <strong>{source.title}</strong>
                          <small>{source.url}</small>
                        </span>
                        <ExternalLink size={16} />
                      </button>
                    ))}
                  </div>
                </section>
              )}
              {!hasExploreContent && (
                <div className="detail-empty">这份总结没有使用联网资料</div>
              )}
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

function FormulaExpression({ value }: { value: string }) {
  const original = value.trim();
  const expression = original.startsWith("$$") && original.endsWith("$$")
    ? original.slice(2, -2).trim()
    : original.startsWith("$") && original.endsWith("$")
      ? original.slice(1, -1).trim()
      : original;
  const isLatex = original.startsWith("$") || /\\[a-zA-Z]+|[{}]/.test(expression);
  return (
    <div className={`formula-expression${isLatex ? " latex" : " plain"}`}>
      {isLatex ? <RichMessage content={`$$\n${expression}\n$$`} /> : expression}
    </div>
  );
}

function fileToBase64(file: File, maxBytes = 50 * 1024 * 1024) {
  if (file.size <= 0) return Promise.reject(new Error("文件内容为空"));
  if (file.size > maxBytes) {
    return Promise.reject(new Error(`文件不能超过 ${Math.round(maxBytes / 1024 / 1024)} MB`));
  }
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("无法读取 Slides 文件"));
    reader.onload = () => {
      const value = String(reader.result ?? "");
      resolve(value.slice(value.indexOf(",") + 1));
    };
    reader.readAsDataURL(file);
  });
}

function ChatPanel({
  courseId,
  lectureId,
  preferences,
  onCitation,
}: {
  courseId: number | null;
  lectureId: number | null;
  preferences: SummaryPreferences;
  onCitation: (citation: ChatCitation) => void;
}) {
  const [scope, setScope] = useState<ChatScope>(lectureId === null ? "course" : "lecture");
  const [webSearch, setWebSearch] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [question, setQuestion] = useState("");
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const streamRef = useRef<HTMLDivElement>(null);
  const historyRequestRef = useRef(0);
  const contextKey = `${courseId ?? "none"}:${lectureId ?? "none"}:${scope}`;
  const contextKeyRef = useRef(contextKey);
  contextKeyRef.current = contextKey;

  useEffect(() => {
    if (lectureId === null && scope === "lecture") setScope("course");
  }, [lectureId, scope]);

  const loadHistory = useCallback(async () => {
    const requestedContext = `${courseId ?? "none"}:${lectureId ?? "none"}:${scope}`;
    if (requestedContext !== contextKeyRef.current) return;
    const requestId = ++historyRequestRef.current;
    if (!isTauri() || courseId === null || (scope === "lecture" && lectureId === null)) {
      if (requestedContext === contextKeyRef.current) setMessages([]);
      return;
    }
    try {
      const history = await invoke<{ messages: ChatMessage[] }>("get_chat_history", {
        courseId,
        lectureId: scope === "lecture" ? lectureId : null,
        scope,
      });
      if (requestId === historyRequestRef.current && requestedContext === contextKeyRef.current) {
        setMessages(history.messages);
        setError(null);
      }
    } catch (reason) {
      if (requestId === historyRequestRef.current && requestedContext === contextKeyRef.current) {
        setError(String(reason));
      }
    }
  }, [courseId, lectureId, scope]);

  useEffect(() => { void loadHistory(); }, [loadHistory]);
  useEffect(() => {
    streamRef.current?.scrollTo({ top: streamRef.current.scrollHeight });
  }, [asking, messages]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const content = question.trim();
    if (!content || courseId === null || asking) return;
    const requestedContext = contextKeyRef.current;
    const optimistic: ChatMessage = {
      id: -Date.now(),
      threadId: -1,
      role: "user",
      content,
      citations: [],
      createdAt: Date.now(),
    };
    setMessages((current) => [...current, optimistic]);
    setQuestion("");
    setAsking(true);
    setError(null);
    try {
      await invoke<ChatAnswer>("ask_lecture_chat", {
        request: {
          courseId,
          lectureId: scope === "lecture" ? lectureId : null,
          scope,
          question: content,
          provider: preferences.textProvider,
          workspaceId: preferences.alibabaWorkspaceId,
          model: preferences.lectureSummaryModel,
          webSearch,
        },
      });
      if (requestedContext === contextKeyRef.current) await loadHistory();
    } catch (reason) {
      if (requestedContext === contextKeyRef.current) {
        setMessages((current) => current.filter((message) => message.id !== optimistic.id));
        setQuestion(content);
        setError(String(reason));
      }
    } finally {
      setAsking(false);
    }
  };

  return (
    <div className="chat-panel">
      <div className="chat-controls">
        <div className="chat-scope-control" aria-label="问答资料范围">
          <button type="button" className={scope === "lecture" ? "active" : ""} disabled={lectureId === null || asking} onClick={() => setScope("lecture")}>本节课</button>
          <button type="button" className={scope === "course" ? "active" : ""} disabled={asking} onClick={() => setScope("course")}>本课程</button>
        </div>
        <label className={`chat-web-toggle${asking ? " disabled" : ""}`}>
          <input type="checkbox" checked={webSearch} disabled={asking} onChange={(event) => setWebSearch(event.target.checked)} />
          <Globe2 size={14} />
          联网核验
        </label>
      </div>
      <div className="chat-stream" ref={streamRef}>
        {messages.length ? messages.map((message) => (
          <article className={`chat-message ${message.role}`} key={message.id}>
            <div className="chat-message-copy">
              {message.role === "assistant" ? <RichMessage content={message.content} /> : message.content}
            </div>
            {!!message.citations.length && (
              <div className="chat-citations">
                {message.citations.map((citation) => (
                  <button type="button" key={`${message.id}-${citation.id}`} onClick={() => onCitation(citation)}>
                    <span>{citation.id}</span>
                    {citation.title}
                    {citation.kind === "web" && <ExternalLink size={12} />}
                  </button>
                ))}
              </div>
            )}
          </article>
        )) : (
          <div className="chat-empty">
            <BrainCircuit size={25} />
            <strong>向课堂助理提问</strong>
            <span>{lectureId === null ? "选择历史课堂，或询问整门课程" : "回答会引用课堂时间、Slides 页码或网页来源"}</span>
          </div>
        )}
        {asking && <div className="chat-thinking"><LoaderCircle className="spin" size={16} />正在检索并核验出处</div>}
      </div>
      {error && <div className="chat-error">{error}</div>}
      <form className="chat-composer" onSubmit={submit}>
        <textarea
          rows={2}
          maxLength={8_000}
          value={question}
          placeholder="询问概念、例题或比较不同教学周内容"
          onChange={(event) => setQuestion(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              event.currentTarget.form?.requestSubmit();
            }
          }}
        />
        <button className="icon-button" type="submit" title="发送" disabled={!question.trim() || asking || courseId === null}>
          {asking ? <LoaderCircle className="spin" size={17} /> : <Send size={17} />}
        </button>
      </form>
    </div>
  );
}

function LectureSetupDialog({
  open,
  course,
  starting,
  onClose,
  onStart,
}: {
  open: boolean;
  course: Course | null;
  starting: boolean;
  onClose: () => void;
  onStart: (options: StartLectureOptions) => Promise<void>;
}) {
  const [title, setTitle] = useState("");
  const [slides, setSlides] = useState<File | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    const today = new Date().toLocaleDateString("zh-CN");
    setTitle(`${course?.name || "NUS Lecture"} · ${today}`);
    setSlides(null);
    setError(null);
  }, [course?.name, open]);

  if (!open) return null;
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setError(null);
    try {
      const options: StartLectureOptions = { title: title.trim() };
      if (slides) {
        if (!isSupportedSlidesFileName(slides.name)) {
          throw new Error("目前支持 PDF、PPTX 和 PowerPoint 97-2003 PPT");
        }
        options.slides = {
          name: slides.name,
          mimeType: slidesMimeType(slides.name, slides.type),
          dataBase64: await fileToBase64(slides),
          parsedPages: isLegacyPowerPointFileName(slides.name)
            ? await parseLegacyPowerPointPages(slides)
            : undefined,
        };
      }
      await onStart(options);
    } catch (reason) {
      setError(String(reason));
    }
  };

  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={starting ? undefined : onClose}>
      <section className="settings-dialog lecture-setup-dialog" role="dialog" aria-modal="true" onMouseDown={(event) => event.stopPropagation()}>
        <header className="dialog-header">
          <div><p className="eyebrow">新课堂</p><h2>准备本周课堂</h2></div>
          <button className="icon-button" type="button" onClick={onClose} title="关闭" disabled={starting}><X size={19} /></button>
        </header>
        <form className="lecture-setup-form" onSubmit={submit}>
          <label>
            <span>课堂标题</span>
            <input value={title} onChange={(event) => setTitle(event.target.value)} required />
          </label>
          <label className={`slides-dropzone ${slides ? "has-file" : ""}`}>
            <input
              type="file"
              accept=".pdf,.pptx,.ppt,application/pdf,application/vnd.openxmlformats-officedocument.presentationml.presentation,application/vnd.ms-powerpoint"
              onChange={(event) => setSlides(event.target.files?.[0] ?? null)}
            />
            {slides ? <FileText size={24} /> : <Upload size={24} />}
            <strong>{slides?.name || "选择本节课 Slides"}</strong>
            <span>{slides ? `${(slides.size / 1024 / 1024).toFixed(1)} MB` : "PDF、PPTX 或 PPT，可暂不上传"}</span>
          </label>
          <div className="syllabus-preview">
            <strong>课程 Syllabus</strong>
            <p>{course?.description || "尚未填写，将仅使用 Slides 和课堂实时内容。"}</p>
          </div>
          {error && <div className="form-error">{error}</div>}
          <div className="dialog-actions">
            <button className="secondary-button" type="button" onClick={onClose} disabled={starting}>取消</button>
            <button className="primary-button" type="submit" disabled={starting}>
              {starting ? <LoaderCircle className="spin" size={17} /> : <Mic size={17} />}
              {starting ? "正在解析并连接" : "开始听课"}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}

function HistoryDialog({
  open,
  course,
  lectures,
  loading,
  onClose,
  onOpenLecture,
  onDeleteLecture,
  onBackup,
  onRestore,
  onBackfillSummaries,
  backfillProgress,
}: {
  open: boolean;
  course: Course | null;
  lectures: LectureListItem[];
  loading: boolean;
  onClose: () => void;
  onOpenLecture: (lecture: LectureListItem) => void;
  onDeleteLecture: (lecture: LectureListItem) => void;
  onBackup: () => Promise<void>;
  onRestore: (file: File) => Promise<void>;
  onBackfillSummaries: () => Promise<void>;
  backfillProgress: string | null;
}) {
  if (!open) return null;
  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        className="settings-dialog history-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="history-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="dialog-header">
          <div>
            <p className="eyebrow">{course?.code || "COURSE"}</p>
            <h2 id="history-title">{course?.name || "课堂"} · 历史记录</h2>
          </div>
          <button className="icon-button" type="button" onClick={onClose} title="关闭">
            <X size={19} />
          </button>
        </header>
        <div className="history-list">
          {loading ? (
            <div className="history-empty"><LoaderCircle className="spin" size={20} />正在读取课堂</div>
          ) : lectures.length ? lectures.map((lecture, index) => (
            <div className="history-item" key={lecture.id}>
              <button
                className="history-open-button"
                type="button"
                onClick={() => onOpenLecture(lecture)}
              >
                <span className="history-week">W{lectures.length - index}</span>
                <span className="history-main">
                  <strong>{lecture.title}</strong>
                  <small>
                    {new Date(lecture.startedAt).toLocaleString("zh-CN", {
                      month: "short",
                      day: "numeric",
                      hour: "2-digit",
                      minute: "2-digit",
                    })}
                    {lecture.endedAt
                      ? ` · 结束 ${new Date(lecture.endedAt).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}`
                      : " · 未正常结束"}
                  </small>
                </span>
                <span className="history-stats">
                  <small><Clock3 size={11} />时长 {formatTime(lecture.elapsedMs)}</small>
                  <small>{lecture.segmentCount} 条记录 · {lecture.summaryCount} 份总结</small>
                  <small>{lecture.documentCount} 份 Slides · {lecture.bookmarkCount} 个标记</small>
                </span>
                <ChevronDown size={16} />
              </button>
              <button
                className="icon-button history-delete-button"
                type="button"
                title="删除课堂记录"
                aria-label={`删除 ${lecture.title}`}
                disabled={lecture.status !== "ended"}
                onClick={() => onDeleteLecture(lecture)}
              >
                <Trash2 size={16} />
              </button>
            </div>
          )) : (
            <div className="history-empty"><History size={23} />这门课还没有保存的课堂</div>
          )}
        </div>
        <footer className="history-actions">
          <button
            className="secondary-button"
            type="button"
            disabled={loading || Boolean(backfillProgress)}
            onClick={() => void onBackfillSummaries()}
          >
            {backfillProgress
              ? <LoaderCircle className="spin" size={15} />
              : <RotateCcw size={15} />}
            {backfillProgress || "补齐整课总结"}
          </button>
          <label className="secondary-button">
            <Upload size={15} />
            恢复备份
            <input type="file" accept=".nuslecture" onChange={(event) => {
              const file = event.target.files?.[0];
              if (file) void onRestore(file);
              event.currentTarget.value = "";
            }} />
          </label>
          <button className="secondary-button" type="button" onClick={() => void onBackup()}>
            <Download size={15} />
            备份全部数据
          </button>
        </footer>
      </section>
    </div>
  );
}

function DeleteLectureDialog({
  lecture,
  deleting,
  onClose,
  onConfirm,
}: {
  lecture: LectureListItem | null;
  deleting: boolean;
  onClose: () => void;
  onConfirm: () => Promise<void>;
}) {
  if (!lecture) return null;
  return (
    <div className="dialog-backdrop delete-dialog-backdrop" role="presentation" onMouseDown={deleting ? undefined : onClose}>
      <section className="settings-dialog delete-lecture-dialog" role="alertdialog" aria-modal="true" aria-labelledby="delete-lecture-title" onMouseDown={(event) => event.stopPropagation()}>
        <header className="dialog-header">
          <div><p className="eyebrow">删除课堂</p><h2 id="delete-lecture-title">确认删除这条历史记录？</h2></div>
          <button className="icon-button" type="button" title="关闭" onClick={onClose} disabled={deleting}><X size={19} /></button>
        </header>
        <div className="delete-lecture-content">
          <strong>{lecture.title}</strong>
          <div className="delete-lecture-metrics">
            <span><Clock3 size={14} />{formatTime(lecture.elapsedMs)}</span>
            <span>{lecture.segmentCount} 条记录</span>
            <span>{lecture.summaryCount} 份总结</span>
            <span>{lecture.documentCount} 份 Slides</span>
          </div>
          <p>删除后，相关转写、总结、标记、Slides 和本节课问答都会一并移除，且无法撤销。</p>
        </div>
        <footer className="dialog-actions">
          <button className="secondary-button" type="button" onClick={onClose} disabled={deleting}>取消</button>
          <button className="stop-button" type="button" onClick={() => void onConfirm()} disabled={deleting}>
            {deleting ? <LoaderCircle className="spin" size={17} /> : <Trash2 size={17} />}
            {deleting ? "正在删除" : "永久删除"}
          </button>
        </footer>
      </section>
    </div>
  );
}

function SlideDetailDialog({
  chunk,
  pageCount,
  hasPrevious,
  hasNext,
  isCurrent,
  onNavigate,
  onJump,
  onSetCurrent,
  onClose,
}: {
  chunk: DocumentChunk | null;
  pageCount: number;
  hasPrevious: boolean;
  hasNext: boolean;
  isCurrent: boolean;
  onNavigate: (offset: number) => void;
  onJump: (pageNumber: number) => void;
  onSetCurrent: () => void;
  onClose: () => void;
}) {
  const [source, setSource] = useState<DocumentSource | null>(null);
  const [sourceError, setSourceError] = useState("");
  const [sourceLoading, setSourceLoading] = useState(false);

  useEffect(() => {
    const documentId = chunk?.documentId;
    if (!documentId) {
      setSource(null);
      setSourceError("");
      setSourceLoading(false);
      return;
    }
    if (!isTauri()) {
      setSource(null);
      setSourceError("浏览器演示模式无法读取本机 Slides 原文件");
      setSourceLoading(false);
      return;
    }
    let cancelled = false;
    setSource(null);
    setSourceError("");
    setSourceLoading(true);
    invoke<DocumentSource>("read_document_source", { documentId })
      .then((nextSource) => {
        if (!cancelled) setSource(nextSource);
      })
      .catch((reason) => {
        if (!cancelled) setSourceError(String(reason));
      })
      .finally(() => {
        if (!cancelled) setSourceLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [chunk?.documentId]);

  if (!chunk) return null;
  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={onClose}>
      <section className="settings-dialog slide-detail-dialog" role="dialog" aria-modal="true" onMouseDown={(event) => event.stopPropagation()}>
        <header className="dialog-header">
          <div><p className="eyebrow">SLIDE {chunk.pageNumber}</p><h2>{chunk.heading || `第 ${chunk.pageNumber} 页`}</h2></div>
          <button className="icon-button" type="button" title="关闭" onClick={onClose}><X size={19} /></button>
        </header>
        <div className="slide-detail-toolbar">
          <div className="slide-page-controls">
            <button className="icon-button" type="button" title="上一页" disabled={!hasPrevious} onClick={() => onNavigate(-1)}><ChevronLeft size={18} /></button>
            <label>
              <span>P.</span>
              <input
                type="number"
                aria-label="Slides 页码"
                min={1}
                max={pageCount}
                value={chunk.pageNumber}
                onChange={(event) => onJump(Number(event.target.value))}
              />
              <span>/ {pageCount}</span>
            </label>
            <button className="icon-button" type="button" title="下一页" disabled={!hasNext} onClick={() => onNavigate(1)}><ChevronRight size={18} /></button>
          </div>
          <button className={isCurrent ? "secondary-button" : "primary-button"} type="button" disabled={isCurrent} onClick={onSetCurrent}>
            {isCurrent ? <Check size={16} /> : <LocateFixed size={16} />}
            {isCurrent ? "当前匹配页" : "设为当前页"}
          </button>
        </div>
        <div className="slide-detail-content">
          {sourceLoading && (
            <div className="document-preview-status" role="status">
              <LoaderCircle className="spin" size={20} />
              <span>正在读取 Slides 原文件</span>
            </div>
          )}
          {source && (
            <DocumentPagePreview
              source={source}
              pageNumber={chunk.pageNumber}
              fallbackText={chunk.content}
            />
          )}
          {!!sourceError && (
            <div className="document-preview-error">
              <FileText size={24} />
              <strong>原页预览暂时无法显示</strong>
              <span>{sourceError}</span>
              {!!chunk.content.trim() && (
                <details>
                  <summary>查看备用文字</summary>
                  <div>{chunk.content}</div>
                </details>
              )}
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

function BookmarkDialog({
  bookmark,
  onClose,
  onSave,
  onDelete,
}: {
  bookmark: LectureBookmark | null;
  onClose: () => void;
  onSave: (note: string) => Promise<void>;
  onDelete: () => Promise<void>;
}) {
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);
  useEffect(() => setNote(bookmark?.note ?? ""), [bookmark]);
  if (!bookmark) return null;
  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={onClose}>
      <section className="settings-dialog bookmark-dialog" role="dialog" aria-modal="true" onMouseDown={(event) => event.stopPropagation()}>
        <header className="dialog-header">
          <div><p className="eyebrow">课堂标记</p><h2>{formatTimestamp(bookmark.timestampMs)}</h2></div>
          <button className="icon-button" type="button" title="关闭" onClick={onClose}><X size={19} /></button>
        </header>
        <div className="bookmark-editor">
          <textarea rows={4} value={note} placeholder="记录疑问、考试提示或需要复习的内容" onChange={(event) => setNote(event.target.value)} autoFocus />
          <div className="dialog-actions">
            <button className="secondary-button danger-quiet" type="button" onClick={() => void onDelete()}><Trash2 size={15} />删除</button>
            <button className="primary-button" type="button" disabled={saving} onClick={() => {
              setSaving(true);
              void onSave(note).finally(() => setSaving(false));
            }}>{saving ? <LoaderCircle className="spin" size={15} /> : <Check size={15} />}保存标记</button>
          </div>
        </div>
      </section>
    </div>
  );
}

function TranscriptRow({
  segment,
  bookmarked,
  highlighted,
  onRetry,
}: {
  segment: TranscriptSegment;
  bookmarked: boolean;
  highlighted: boolean;
  onRetry?: () => void;
}) {
  return (
    <article
      id={`segment-${segment.id}`}
      className={`transcript-row ${segment.state} ${bookmarked ? "bookmarked" : ""} ${highlighted ? "highlighted" : ""}`}
    >
      <time>{formatTimestamp(segment.startMs)}</time>
      <div className="transcript-copy">
        <p className="english-copy">{segment.english}</p>
        {segment.chinese ? (
          <p className="chinese-copy">{segment.chinese}</p>
        ) : segment.state === "translating" ? (
          <span className="translation-pending" aria-label="正在翻译">
            <i />
            <i />
            <i />
          </span>
        ) : null}
      </div>
      {bookmarked && <Bookmark className="row-bookmark" size={15} aria-label="已标记" />}
      {onRetry && segment.state !== "translating" && <button className="transcript-retry-button" type="button" title="重新翻译" onClick={onRetry}><RotateCcw size={13} /></button>}
    </article>
  );
}

export default function App() {
  const [settings, setSettings] = useState<CourseSettings>(emptyCourseSettings);
  const [courses, setCourses] = useState<Course[]>([]);
  const [terms, setTerms] = useState<GlossaryTerm[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [courseMenuOpen, setCourseMenuOpen] = useState(false);
  const [hasOpenAiApiKey, setHasOpenAiApiKey] = useState(false);
  const [hasAlibabaApiKey, setHasAlibabaApiKey] = useState(false);
  const [hasAlibabaAsrApiKey, setHasAlibabaAsrApiKey] = useState(false);
  const [hasDeepgramApiKey, setHasDeepgramApiKey] = useState(false);
  const [appError, setAppError] = useState<string | null>(null);
  const [summaryPreferences, setSummaryPreferences] = useState<SummaryPreferences>(loadSummaryPreferences);
  const [selectedSummary, setSelectedSummary] = useState<TopicSummary | null>(null);
  const [translationSegment, setTranslationSegment] = useState<TranscriptSegment | null>(null);
  const [translationBusy, setTranslationBusy] = useState(false);
  const [translationError, setTranslationError] = useState<string | null>(null);
  const [regeneratingLectureSummary, setRegeneratingLectureSummary] = useState(false);
  const [backfillProgress, setBackfillProgress] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [lectureSetupOpen, setLectureSetupOpen] = useState(false);
  const [startingLecture, setStartingLecture] = useState(false);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [lectures, setLectures] = useState<LectureListItem[]>([]);
  const [lecturePendingDelete, setLecturePendingDelete] = useState<LectureListItem | null>(null);
  const [deletingLecture, setDeletingLecture] = useState(false);
  const [archivedLecture, setArchivedLecture] = useState<LectureSnapshot | null>(null);
  const [bookmarks, setBookmarks] = useState<LectureBookmark[]>([]);
  const [editingBookmark, setEditingBookmark] = useState<LectureBookmark | null>(null);
  const [summaryFollowingLatest, setSummaryFollowingLatest] = useState(true);
  const [unreadSummaries, setUnreadSummaries] = useState(0);
  const [followingLatest, setFollowingLatest] = useState(true);
  const [unreadSegments, setUnreadSegments] = useState(0);
  const [highlightedSegmentId, setHighlightedSegmentId] = useState<string | null>(null);
  const [leftPaneTab, setLeftPaneTab] = useState<"summary" | "chat">("summary");
  const [selectedDocumentChunk, setSelectedDocumentChunk] = useState<DocumentChunk | null>(null);
  const [selectedDocumentChunks, setSelectedDocumentChunks] = useState<DocumentChunk[]>([]);
  const [lectureDocuments, setLectureDocuments] = useState<LectureDocument[]>([]);
  const [slideMatch, setSlideMatch] = useState<SlideMatch | null>(null);
  const summaryPaneRef = useRef<HTMLDivElement>(null);
  const summaryFollowingLatestRef = useRef(true);
  const previousSummaryCountRef = useRef(0);
  const transcriptPaneRef = useRef<HTMLDivElement>(null);
  const followingLatestRef = useRef(true);
  const previousSegmentCountRef = useRef(0);
  const slideMatchRef = useRef<SlideMatch | null>(null);
  const pendingSlideCandidateRef = useRef<{ key: string; count: number } | null>(null);
  const lastSlideQueryRef = useRef("");
  const slideRequestRef = useRef(0);
  const session = useLectureSession(settings, summaryPreferences);
  const [showPerformanceDetails, setShowPerformanceDetails] = useState(false);
  const asrModel = summaryPreferences.transcriptionModel;
  const translationModel = summaryPreferences.translationProvider === "local"
    ? summaryPreferences.localTranslationModel
    : summaryPreferences.translationModel;
  const restoreLecture = session.restoreLecture;
  const restoreLectureRef = useRef(restoreLecture);
  restoreLectureRef.current = restoreLecture;

  const isActive = ["connecting", "live", "paused", "demo"].includes(session.status);
  const canPause = ["live", "paused"].includes(session.status);
  const courseLocked = ["connecting", "live", "paused", "recovered", "error"].includes(session.status);
  const currentCourse = courses.find((course) => course.id === settings.courseId) ?? null;
  const displaySegments = archivedLecture?.segments ?? session.segments;
  const displaySummaries = archivedLecture?.summaries ?? session.summaries;
  const displayElapsedMs = archivedLecture?.elapsedMs ?? session.elapsedMs;
  const displayLectureId = archivedLecture?.lectureId ?? session.lectureId;
  const displayLectureNeedsSummary = Boolean(
    !isActive
    && displayLectureId !== null
    && displaySegments.some((segment) => segment.state !== "interim" && segment.english.trim())
    && !displaySummaries.some((summary) => summary.kind === "lecture"),
  );
  const isViewingHistory = archivedLecture !== null;

  const retryArchivedTranslation = async () => {
    if (!translationSegment || !archivedLecture || translationBusy) return;
    setTranslationBusy(true);
    setTranslationError(null);
    try {
      const update = (next: TranscriptSegment) => {
        setTranslationSegment(next);
        setArchivedLecture((current) => current ? { ...current, segments: current.segments.map((item) => item.id === next.id ? next : item) } : current);
      };
      const updated = await translateArchivedSegment(translationSegment, archivedLecture.segments, settings, summaryPreferences, update);
      const segments = archivedLecture.segments.map((item) => item.id === updated.id ? updated : item);
      await invoke("save_lecture_snapshot", { snapshot: { lectureId: archivedLecture.lectureId, elapsedMs: archivedLecture.elapsedMs, status: archivedLecture.status, segments, summaries: archivedLecture.summaries } });
      setArchivedLecture((current) => current ? { ...current, segments } : current);
      setTranslationSegment(null);
    } catch (reason) { setTranslationError(String(reason)); }
    finally { setTranslationBusy(false); }
  };

  const commitSlideMatch = useCallback((match: SlideMatch | null) => {
    slideMatchRef.current = match;
    setSlideMatch(match);
  }, []);

  const applyCourse = useCallback((course: Course, glossaryTerms: GlossaryTerm[]) => {
    setTerms(glossaryTerms);
    setSettings(settingsForCourse(course, glossaryTerms));
    setArchivedLecture(null);
    setBookmarks([]);
    setSelectedSummary(null);
    setSelectedDocumentChunk(null);
    setSelectedDocumentChunks([]);
    setLectureDocuments([]);
    commitSlideMatch(null);
    summaryFollowingLatestRef.current = true;
    setSummaryFollowingLatest(true);
    setUnreadSummaries(0);
    followingLatestRef.current = true;
    setFollowingLatest(true);
    setUnreadSegments(0);
    localStorage.setItem("lecture-assistant-course-id", String(course.id));
  }, [commitSlideMatch]);

  const fetchCourse = useCallback(async (courseId: number) => {
    if (!isTauri()) {
      const course = courses.find((item) => item.id === courseId);
      if (!course) throw new Error("未找到这门课程");
      return { course, terms };
    }
    const courseRows = await invoke<Course[]>("list_courses");
    const course = courseRows.find((item) => item.id === courseId);
    if (!course) throw new Error("未找到这门课程");
    const storedTerms = await invoke<GlossaryTerm[]>("list_glossary_terms", { courseId });
    const glossaryTerms = await ensureRecommendedGlossary(course, storedTerms);
    return { course, terms: glossaryTerms };
  }, [courses, terms]);

  useEffect(() => {
    if (!isTauri()) {
      const legacy = loadLegacyCourse();
      const previewCourse: Course = {
        id: 0,
        ...legacy.course,
        createdAt: Date.now(),
      };
      setCourses([previewCourse]);
      applyCourse(previewCourse, legacy.terms);
      return;
    }

    let cancelled = false;
    const bootstrap = async () => {
      try {
        void Promise.all([
          invoke<boolean>("has_provider_api_key", { provider: "openai" }),
          invoke<boolean>("has_provider_api_key", { provider: "alibaba" }),
          invoke<boolean>("has_provider_api_key", { provider: "alibaba-asr" }),
          invoke<boolean>("has_provider_api_key", { provider: "deepgram" }),
        ]).then(([openAiKeySaved, alibabaKeySaved, alibabaAsrKeySaved, deepgramKeySaved]) => {
          if (cancelled) return;
          setHasOpenAiApiKey(openAiKeySaved);
          setHasAlibabaApiKey(alibabaKeySaved);
          setHasAlibabaAsrApiKey(alibabaAsrKeySaved);
          setHasDeepgramApiKey(deepgramKeySaved);
        }).catch(() => undefined);

        let courseRows = await invoke<Course[]>("list_courses");
        if (!courseRows.length) {
          const legacy = loadLegacyCourse();
          const created = await invoke<Course>("save_course", { course: legacy.course });
          await invoke("replace_glossary_terms", {
            courseId: created.id,
            terms: legacy.terms,
          });
          courseRows = [created];
        }

        const recoverable = await invoke<LectureSnapshot | null>("get_recoverable_lecture");
        const savedId = Number(localStorage.getItem("lecture-assistant-course-id"));
        const selected = recoverable
          ? courseRows.find((course) => course.id === recoverable.courseId)
          : courseRows.find((course) => course.id === savedId);
        const course = selected ?? courseRows[0];
        const storedTerms = await invoke<GlossaryTerm[]>("list_glossary_terms", {
          courseId: course.id,
        });
        const glossaryTerms = await ensureRecommendedGlossary(course, storedTerms);

        if (cancelled) return;
        setCourses(courseRows);
        applyCourse(course, glossaryTerms);
        if (recoverable) {
          restoreLectureRef.current(recoverable);
          setBookmarks(await invoke<LectureBookmark[]>("list_lecture_bookmarks", {
            lectureId: recoverable.lectureId,
          }));
        }
      } catch (reason) {
        if (!cancelled) setAppError(`无法加载本地课堂数据：${String(reason)}`);
      }
    };
    void bootstrap();
    return () => {
      cancelled = true;
    };
  }, [applyCourse]);

  useEffect(() => {
    const pane = summaryPaneRef.current;
    const added = Math.max(0, displaySummaries.length - previousSummaryCountRef.current);
    previousSummaryCountRef.current = displaySummaries.length;
    if (!pane || leftPaneTab !== "summary") return;
    if (summaryFollowingLatestRef.current && !isViewingHistory) {
      window.requestAnimationFrame(() => pane.scrollTo({ top: pane.scrollHeight }));
      setUnreadSummaries(0);
    } else if (added > 0 && !isViewingHistory) {
      setUnreadSummaries((current) => current + added);
    }
  }, [displaySummaries, isViewingHistory, leftPaneTab]);

  useEffect(() => {
    const pane = transcriptPaneRef.current;
    const added = Math.max(0, displaySegments.length - previousSegmentCountRef.current);
    previousSegmentCountRef.current = displaySegments.length;
    if (!pane) return;
    if (followingLatestRef.current && !isViewingHistory) {
      window.requestAnimationFrame(() => pane.scrollTo({ top: pane.scrollHeight }));
      setUnreadSegments(0);
    } else if (added > 0 && !isViewingHistory) {
      setUnreadSegments((current) => current + added);
    }
  }, [displaySegments, isViewingHistory]);

  useEffect(() => {
    commitSlideMatch(null);
    pendingSlideCandidateRef.current = null;
    lastSlideQueryRef.current = "";
    slideRequestRef.current += 1;
  }, [commitSlideMatch, displayLectureId]);

  useEffect(() => {
    setLectureDocuments([]);
    if (!isTauri() || displayLectureId === null) return;
    let cancelled = false;
    const loadDocuments = () => {
      void invoke<LectureDocument[]>("list_lecture_documents", {
        lectureId: displayLectureId,
      }).then((documents) => {
        if (!cancelled) setLectureDocuments(documents);
      }).catch(() => undefined);
    };
    loadDocuments();
    const retry = window.setTimeout(loadDocuments, 2_000);
    return () => {
      cancelled = true;
      window.clearTimeout(retry);
    };
  }, [displayLectureId]);

  useEffect(() => {
    if (!isTauri() || displayLectureId === null) {
      commitSlideMatch(null);
      return;
    }
    const stableSegments = displaySegments
      .filter((segment) => segment.state !== "interim" && segment.english.trim());
    const latest = stableSegments[stableSegments.length - 1];
    if (!latest) return;
    const querySignature = `${latest.id}:${latest.english}`;
    if (querySignature === lastSlideQueryRef.current) return;
    lastSlideQueryRef.current = querySignature;
    const recent = stableSegments
      .filter((segment) => latest.startMs - segment.startMs <= 35_000)
      .slice(-16)
      .map((segment) => segment.english)
      .join(" ");
    if (!recent) return;
    const timer = window.setTimeout(() => {
      const requestId = ++slideRequestRef.current;
      const current = slideMatchRef.current;
      void invoke<SlideMatch | null>("match_lecture_slide", {
        lectureId: displayLectureId,
        query: recent,
        latestText: stableSegments.slice(-3).map((segment) => segment.english).join(" "),
        currentDocumentId: current?.documentId ?? null,
        currentPage: current?.pageNumber ?? null,
      }).then((candidate) => {
        if (requestId !== slideRequestRef.current || !candidate) {
          if (!candidate) pendingSlideCandidateRef.current = null;
          return;
        }
        const latestMatch = slideMatchRef.current;
        const candidateKey = `${candidate.documentId}:${candidate.pageNumber}`;
        if (latestMatch && candidate.documentId === latestMatch.documentId && candidate.pageNumber === latestMatch.pageNumber) {
          pendingSlideCandidateRef.current = null;
          commitSlideMatch(candidate);
          return;
        }
        const pending = pendingSlideCandidateRef.current;
        const confirmations = pending?.key === candidateKey ? pending.count + 1 : 1;
        pendingSlideCandidateRef.current = { key: candidateKey, count: confirmations };
        const requiredConfirmations = requiredSlideConfirmations(
          latestMatch?.documentId ?? null,
          latestMatch?.pageNumber ?? null,
          candidate.documentId,
          candidate.pageNumber,
        );
        if (candidate.confidence >= 0.9 || confirmations >= requiredConfirmations) {
          pendingSlideCandidateRef.current = null;
          commitSlideMatch(candidate);
        }
      }).catch(() => undefined);
    }, 900);
    return () => window.clearTimeout(timer);
  }, [commitSlideMatch, displayLectureId, displaySegments]);

  const handleSummaryScroll = () => {
    const pane = summaryPaneRef.current;
    if (!pane || isViewingHistory) return;
    const atBottom = isNearScrollBottom(pane);
    if (atBottom !== summaryFollowingLatestRef.current) {
      summaryFollowingLatestRef.current = atBottom;
      setSummaryFollowingLatest(atBottom);
    }
    if (atBottom) setUnreadSummaries(0);
  };

  const jumpToLatestSummary = () => {
    const pane = summaryPaneRef.current;
    if (!pane) return;
    summaryFollowingLatestRef.current = true;
    setSummaryFollowingLatest(true);
    setUnreadSummaries(0);
    pane.scrollTo({ top: pane.scrollHeight, behavior: "smooth" });
  };

  const handleTranscriptScroll = () => {
    const pane = transcriptPaneRef.current;
    if (!pane || isViewingHistory) return;
    const atBottom = isNearScrollBottom(pane);
    if (atBottom !== followingLatestRef.current) {
      followingLatestRef.current = atBottom;
      setFollowingLatest(atBottom);
    }
    if (atBottom) setUnreadSegments(0);
  };

  const jumpToLatest = () => {
    const pane = transcriptPaneRef.current;
    if (!pane) return;
    followingLatestRef.current = true;
    setFollowingLatest(true);
    setUnreadSegments(0);
    pane.scrollTo({ top: pane.scrollHeight, behavior: "smooth" });
  };

  const loadLectureHistory = useCallback(async () => {
    if (!isTauri() || settings.courseId === null) return;
    setHistoryLoading(true);
    try {
      setLectures(await invoke<LectureListItem[]>("list_lectures", { courseId: settings.courseId }));
    } catch (reason) {
      setAppError(`无法读取历史课堂：${String(reason)}`);
    } finally {
      setHistoryLoading(false);
    }
  }, [settings.courseId]);

  const openHistory = () => {
    setHistoryOpen(true);
    void loadLectureHistory();
  };

  const revealSavedFile = async (path: string, label: string) => {
    try {
      await revealItemInDir(path);
    } catch (reason) {
      setAppError(`${label}已保存到 ${path}，但无法在文件管理器中定位：${String(reason)}`);
    }
  };

  const backupAllData = async () => {
    if (!isTauri()) return;
    try {
      const path = await invoke<string>("create_app_backup");
      await revealSavedFile(path, "备份");
    } catch (reason) {
      setAppError(`备份失败：${String(reason)}`);
    }
  };

  const restoreBackup = async (file: File) => {
    if (!isTauri()) return;
    try {
      const message = await invoke<string>("stage_app_restore", {
        dataBase64: await fileToBase64(file, 500 * 1024 * 1024),
      });
      setAppError(`${message}。请关闭并重新打开应用。`);
    } catch (reason) {
      setAppError(`恢复备份失败：${String(reason)}`);
    }
  };

  const exportDisplayedLecture = async () => {
    if (!isTauri() || displayLectureId === null) return;
    try {
      const path = await invoke<string>("export_lecture_markdown", {
        lectureId: displayLectureId,
      });
      await revealSavedFile(path, "课堂导出");
    } catch (reason) {
      setAppError(`导出课堂失败：${String(reason)}`);
    }
  };

  const generateAndSaveLectureSummary = async (lecture: LectureSnapshot) => {
    const candidates = lecture.segments.filter(
      (segment) => segment.state !== "interim" && segment.english.trim(),
    );
    if (!candidates.length) {
      throw new Error("这节历史课堂没有可用于重新总结的稳定转写");
    }
    const existingLectureSummary = lecture.summaries.find(
        (summary) => summary.kind === "lecture",
    );
    const topicSummaries = lecture.summaries.filter(
      (summary) => summary.kind !== "lecture",
    );
    const summary = await invoke<TopicSummary>("generate_topic_summary", {
      request: {
        summaryId: existingLectureSummary?.id ?? `lecture-regenerated-${Date.now()}`,
        kind: "lecture",
        startMs: candidates[0].startMs,
        endMs: candidates[candidates.length - 1].startMs,
        courseName: settings.courseName,
        courseContext: settings.courseContext,
        glossary: settings.glossary,
        transcript: buildLectureSummaryInput(candidates, topicSummaries),
        provider: summaryPreferences.textProvider,
        workspaceId: summaryPreferences.alibabaWorkspaceId,
        model: summaryModelForKind(summaryPreferences, "lecture"),
        webSearch: summaryPreferences.webSearchEnabled,
      },
    });
    const summaries = [...topicSummaries, summary];
    await invoke("save_lecture_snapshot", {
      snapshot: {
        lectureId: lecture.lectureId,
        elapsedMs: lecture.elapsedMs,
        status: lecture.status,
        segments: lecture.segments,
        summaries,
      },
    });
    return { summary, summaries };
  };

  const regenerateArchivedLectureSummary = async () => {
    if (!isTauri() || displayLectureId === null || regeneratingLectureSummary) return;
    setRegeneratingLectureSummary(true);
    try {
      const lecture = archivedLecture ?? await invoke<LectureSnapshot>("get_lecture", {
        lectureId: displayLectureId,
      });
      const { summary, summaries } = await generateAndSaveLectureSummary(lecture);
      setArchivedLecture({ ...lecture, summaries });
      setSelectedSummary(summary);
    } catch (reason) {
      setAppError(`重新生成整课总结失败：${String(reason)}`);
    } finally {
      setRegeneratingLectureSummary(false);
    }
  };

  const backfillCourseLectureSummaries = async () => {
    if (!isTauri() || backfillProgress) return;
    const eligible = lectures.filter(
      (lecture) => lecture.status === "ended" && lecture.elapsedMs >= 30 * 60_000,
    );
    setBackfillProgress("正在检查课堂");
    let completed = 0;
    const failures: string[] = [];
    try {
      for (let index = 0; index < eligible.length; index += 1) {
        const item = eligible[index];
        try {
          const lecture = await invoke<LectureSnapshot>("get_lecture", { lectureId: item.id });
          if (lecture.summaries.some((summary) => summary.kind === "lecture")) continue;
          setBackfillProgress(`生成中 ${index + 1}/${eligible.length}`);
          await generateAndSaveLectureSummary(lecture);
          completed += 1;
        } catch (reason) {
          failures.push(`${item.title}：${String(reason)}`);
        }
      }
      await loadLectureHistory();
      if (failures.length) {
        setAppError(`已补生成 ${completed} 节，${failures.length} 节失败：${failures.join("；")}`);
      } else if (completed) {
        setAppError(`已补生成 ${completed} 节整课总结`);
      } else {
        setAppError("这门课没有需要补生成的完整课堂");
      }
    } finally {
      setBackfillProgress(null);
    }
  };

  const openArchivedLecture = async (lecture: LectureListItem) => {
    if (!isTauri()) return;
    setHistoryLoading(true);
    try {
      const [snapshot, savedBookmarks] = await Promise.all([
        invoke<LectureSnapshot>("get_lecture", { lectureId: lecture.id }),
        invoke<LectureBookmark[]>("list_lecture_bookmarks", { lectureId: lecture.id }),
      ]);
      setArchivedLecture(snapshot);
      setBookmarks(savedBookmarks);
      setHistoryOpen(false);
      summaryFollowingLatestRef.current = false;
      setSummaryFollowingLatest(false);
      setUnreadSummaries(0);
      followingLatestRef.current = false;
      setFollowingLatest(false);
      setUnreadSegments(0);
      window.requestAnimationFrame(() => {
        summaryPaneRef.current?.scrollTo({ top: 0 });
        transcriptPaneRef.current?.scrollTo({ top: 0 });
      });
    } catch (reason) {
      setAppError(`无法打开课堂：${String(reason)}`);
    } finally {
      setHistoryLoading(false);
    }
  };

  const returnToCurrentLecture = async () => {
    const currentLectureId = session.lectureId;
    setArchivedLecture(null);
    setSelectedSummary(null);
    setSelectedDocumentChunk(null);
    setSelectedDocumentChunks([]);
    summaryFollowingLatestRef.current = true;
    setSummaryFollowingLatest(true);
    setUnreadSummaries(0);
    followingLatestRef.current = true;
    setFollowingLatest(true);
    setUnreadSegments(0);
    if (isTauri() && currentLectureId !== null) {
      try {
        const currentBookmarks = await invoke<LectureBookmark[]>("list_lecture_bookmarks", {
          lectureId: currentLectureId,
        });
        if (session.lectureId === currentLectureId) setBookmarks(currentBookmarks);
      } catch (reason) {
        setBookmarks([]);
        setAppError(`无法读取当前课堂标记：${String(reason)}`);
      }
    } else {
      setBookmarks([]);
    }
    window.requestAnimationFrame(() => {
      const summaryPane = summaryPaneRef.current;
      const transcriptPane = transcriptPaneRef.current;
      summaryPane?.scrollTo({ top: summaryPane.scrollHeight });
      transcriptPane?.scrollTo({ top: transcriptPane.scrollHeight });
    });
  };

  const deletePendingLecture = async () => {
    if (!isTauri() || !lecturePendingDelete) return;
    setDeletingLecture(true);
    try {
      await invoke<string>("delete_lecture", { lectureId: lecturePendingDelete.id });
      setLectures((current) => current.filter((lecture) => lecture.id !== lecturePendingDelete.id));
      if (archivedLecture?.lectureId === lecturePendingDelete.id) {
        setArchivedLecture(null);
        setBookmarks([]);
        commitSlideMatch(null);
      }
      setLecturePendingDelete(null);
    } catch (reason) {
      setAppError(`删除课堂失败：${String(reason)}`);
    } finally {
      setDeletingLecture(false);
    }
  };

  const jumpToSegment = (segmentId?: string) => {
    if (!segmentId) return;
    setHighlightedSegmentId(segmentId);
    document.getElementById(`segment-${segmentId}`)?.scrollIntoView({
      behavior: "smooth",
      block: "center",
    });
    window.setTimeout(() => setHighlightedSegmentId(null), 1800);
  };

  const openCitation = async (citation: ChatCitation) => {
    if (citation.kind === "web" && citation.url) {
      try {
        if (isTauri()) await openUrl(citation.url);
        else window.open(citation.url, "_blank", "noopener,noreferrer");
      } catch (reason) {
        setAppError(`无法打开网页来源：${String(reason)}`);
      }
      return;
    }
    if (citation.kind === "syllabus") {
      setSettingsOpen(true);
      return;
    }
    if (citation.kind === "slide" && citation.documentId && citation.pageNumber) {
      try {
        const chunks = await invoke<DocumentChunk[]>("list_document_chunks", {
          documentId: citation.documentId,
        });
        setSelectedDocumentChunks(chunks);
        setSelectedDocumentChunk(
          chunks.find((chunk) => chunk.pageNumber === citation.pageNumber) ?? null,
        );
      } catch (reason) {
        setAppError(`无法打开 Slides 引用：${String(reason)}`);
      }
      return;
    }
    if (citation.kind === "transcript" && citation.lectureId && citation.segmentId) {
      if (citation.lectureId !== displayLectureId) {
        try {
          const [snapshot, savedBookmarks] = await Promise.all([
            invoke<LectureSnapshot>("get_lecture", { lectureId: citation.lectureId }),
            invoke<LectureBookmark[]>("list_lecture_bookmarks", { lectureId: citation.lectureId }),
          ]);
          setArchivedLecture(snapshot);
          setBookmarks(savedBookmarks);
        } catch (reason) {
          setAppError(`无法打开课堂引用：${String(reason)}`);
          return;
        }
      }
      window.setTimeout(() => jumpToSegment(citation.segmentId), 50);
    }
  };

  const openMatchedSlide = async () => {
    const documentId = slideMatch?.documentId ?? lectureDocuments[0]?.id;
    if (!documentId) return;
    try {
      const chunks = await invoke<DocumentChunk[]>("list_document_chunks", {
        documentId,
      });
      setSelectedDocumentChunks(chunks);
      setSelectedDocumentChunk(
        chunks.find((chunk) => chunk.pageNumber === slideMatch?.pageNumber) ?? chunks[0] ?? null,
      );
    } catch (reason) {
      setAppError(`无法打开当前 Slide：${String(reason)}`);
    }
  };

  const navigateSelectedSlide = (offset: number) => {
    if (!selectedDocumentChunk) return;
    const index = selectedDocumentChunks.findIndex((chunk) => chunk.id === selectedDocumentChunk.id);
    const next = selectedDocumentChunks[index + offset];
    if (next) setSelectedDocumentChunk(next);
  };

  const jumpToSelectedSlide = (pageNumber: number) => {
    const next = selectedDocumentChunks.find((chunk) => chunk.pageNumber === pageNumber);
    if (next) setSelectedDocumentChunk(next);
  };

  const setSelectedSlideAsCurrent = () => {
    if (!selectedDocumentChunk) return;
    commitSlideMatch({
      documentId: selectedDocumentChunk.documentId,
      documentName: lectureDocuments.find((document) => document.id === selectedDocumentChunk.documentId)?.name
        ?? (slideMatch?.documentId === selectedDocumentChunk.documentId ? slideMatch.documentName : "Slides"),
      pageNumber: selectedDocumentChunk.pageNumber,
      heading: selectedDocumentChunk.heading,
      excerpt: selectedDocumentChunk.content.slice(0, 260),
      score: 100_000,
      confidence: 1,
    });
    pendingSlideCandidateRef.current = null;
  };

  const addBookmark = async () => {
    if (!isTauri() || session.lectureId === null) return;
    const stable = [...session.segments]
      .reverse()
      .find((segment) => segment.state !== "interim" && segment.english.trim());
    try {
      const bookmark = await invoke<LectureBookmark>("add_lecture_bookmark", {
        lectureId: session.lectureId,
        segmentId: stable?.id,
        timestampMs: stable?.startMs ?? session.elapsedMs,
        note: "",
      });
      setBookmarks((current) => [...current, bookmark]);
      jumpToSegment(bookmark.segmentId);
      setEditingBookmark(bookmark);
    } catch (reason) {
      setAppError(`添加标记失败：${String(reason)}`);
    }
  };

  const saveBookmarkNote = async (note: string) => {
    if (!editingBookmark) return;
    try {
      const updated = await invoke<LectureBookmark>("update_lecture_bookmark", {
        bookmarkId: editingBookmark.id,
        note,
      });
      setBookmarks((current) => current.map((bookmark) => bookmark.id === updated.id ? updated : bookmark));
      setEditingBookmark(null);
    } catch (reason) {
      setAppError(`保存标记失败：${String(reason)}`);
    }
  };

  const deleteEditingBookmark = async () => {
    if (!editingBookmark) return;
    try {
      await invoke("delete_lecture_bookmark", { bookmarkId: editingBookmark.id });
      setBookmarks((current) => current.filter((bookmark) => bookmark.id !== editingBookmark.id));
      setEditingBookmark(null);
    } catch (reason) {
      setAppError(`删除标记失败：${String(reason)}`);
    }
  };

  const statusClass = useMemo(() => `status-${session.status}`, [session.status]);

  const selectCourse = async (courseId: number) => {
    if (courseLocked || !isTauri()) {
      setCourseMenuOpen(false);
      return;
    }
    try {
      const selected = await fetchCourse(courseId);
      applyCourse(selected.course, selected.terms);
      setCourseMenuOpen(false);
    } catch (reason) {
      setAppError(String(reason));
    }
  };

  const saveCourse = async (
    draft: CourseDraft,
    nextTerms: GlossaryTerm[],
    openAiApiKey: string,
    alibabaApiKey: string,
    alibabaAsrApiKey: string,
    deepgramApiKey: string,
    nextSummaryPreferences: SummaryPreferences,
  ) => {
    localStorage.setItem(
      "lecture-assistant-summary-preferences",
      JSON.stringify(nextSummaryPreferences),
    );
    setSummaryPreferences(nextSummaryPreferences);
    if (!isTauri()) {
      const previewCourse: Course = {
        id: 0,
        code: draft.code.trim(),
        name: draft.name.trim(),
        description: draft.description.trim(),
        createdAt: Date.now(),
      };
      setCourses([previewCourse]);
      applyCourse(previewCourse, nextTerms);
      if (openAiApiKey.trim() || alibabaApiKey.trim() || alibabaAsrApiKey.trim() || deepgramApiKey.trim()) {
        throw new Error("请在桌面应用中保存 API Key");
      }
      return;
    }

    const saved = await invoke<Course>("save_course", {
      course: {
        id: draft.id,
        code: draft.code.trim(),
        name: draft.name.trim(),
        description: draft.description.trim(),
      },
    });
    const savedTerms = await invoke<GlossaryTerm[]>("replace_glossary_terms", {
      courseId: saved.id,
      terms: nextTerms,
    });
    const courseRows = await invoke<Course[]>("list_courses");
    setCourses(courseRows);
    applyCourse(saved, savedTerms);

    if (openAiApiKey.trim()) {
      await invoke("save_provider_api_key", { provider: "openai", apiKey: openAiApiKey });
      setHasOpenAiApiKey(true);
    }
    if (alibabaApiKey.trim()) {
      await invoke("save_provider_api_key", { provider: "alibaba", apiKey: alibabaApiKey });
      setHasAlibabaApiKey(true);
    }
    if (alibabaAsrApiKey.trim()) {
      await invoke("save_provider_api_key", {
        provider: "alibaba-asr",
        apiKey: alibabaAsrApiKey,
      });
      setHasAlibabaAsrApiKey(true);
    }
    if (deepgramApiKey.trim()) {
      await invoke("save_provider_api_key", { provider: "deepgram", apiKey: deepgramApiKey });
      setHasDeepgramApiKey(true);
    }
  };

  const deleteApiKey = async (provider: "openai" | "alibaba" | "alibaba-asr" | "deepgram") => {
    if (!isTauri()) return;
    await invoke("delete_provider_api_key", { provider });
    if (provider === "openai") setHasOpenAiApiKey(false);
    else if (provider === "alibaba") setHasAlibabaApiKey(false);
    else if (provider === "deepgram") setHasDeepgramApiKey(false);
    else setHasAlibabaAsrApiKey(false);
  };

  const beginLecture = () => {
    const missingAsrKey = summaryPreferences.transcriptionProvider === "openai"
      ? !hasOpenAiApiKey
      : summaryPreferences.transcriptionProvider === "deepgram"
        ? !hasDeepgramApiKey
        : usesAlibabaSingaporeAsr(summaryPreferences.transcriptionModel)
          ? !hasAlibabaApiKey
          : !hasAlibabaAsrApiKey;
    const missingTranslationKey = summaryPreferences.translationProvider === "local"
      ? false
      : summaryPreferences.translationProvider === "openai"
        ? !hasOpenAiApiKey
        : !hasAlibabaApiKey;
    const missingTextKey = summaryPreferences.textProvider === "openai"
      ? !hasOpenAiApiKey
      : !hasAlibabaApiKey;
    if (missingAsrKey || missingTranslationKey || missingTextKey) {
      setSettingsOpen(true);
      return;
    }
    if (session.hasRecoveredLecture || session.status === "error") {
      void session.startLive();
      return;
    }
    setLectureSetupOpen(true);
  };

  const startLectureFromSetup = async (options: StartLectureOptions) => {
    setStartingLecture(true);
    setArchivedLecture(null);
    setBookmarks([]);
    summaryFollowingLatestRef.current = true;
    setSummaryFollowingLatest(true);
    setUnreadSummaries(0);
    followingLatestRef.current = true;
    setFollowingLatest(true);
    setUnreadSegments(0);
    try {
      const createdLectureId = await session.startLive(options);
      if (createdLectureId !== null && isTauri()) {
        setLectureDocuments(await invoke<LectureDocument[]>("list_lecture_documents", {
          lectureId: createdLectureId,
        }).catch(() => []));
      }
      if (createdLectureId !== null) setLectureSetupOpen(false);
    } finally {
      setStartingLecture(false);
    }
  };

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand-block">
          <span className="brand-mark" aria-hidden="true">
            <BookOpen size={19} />
          </span>
          <div>
            <strong>NUS 课堂同传</strong>
            <span>{settings.courseCode || "Lecture workspace"}</span>
          </div>
        </div>

        <div className="course-menu-wrap">
          <button
            className="course-selector"
            type="button"
            aria-expanded={courseMenuOpen}
            onClick={() => setCourseMenuOpen((open) => !open)}
          >
            <span>{settings.courseName}</span>
            <ChevronDown size={16} />
          </button>
          {courseMenuOpen && (
            <div className="course-menu">
              {courses.map((course) => (
                <button
                  type="button"
                  key={course.id}
                  disabled={courseLocked && course.id !== settings.courseId}
                  onClick={() => void selectCourse(course.id)}
                >
                  <span>{course.code || "NUS"}</span>
                  <strong>{course.name}</strong>
                  {course.id === settings.courseId && <Check size={15} />}
                </button>
              ))}
              <button
                className="manage-course-button"
                type="button"
                onClick={() => {
                  setCourseMenuOpen(false);
                  setSettingsOpen(true);
                }}
              >
                <Settings size={15} />
                管理课程与术语
              </button>
            </div>
          )}
        </div>

        <div className="session-meta">
          <div className={`live-status ${statusClass}`}>
            <span className="status-dot" />
            {statusLabels[session.status]}
          </div>
          <div className="timer">
            <Clock3 size={16} />
            <span>{formatTime(displayElapsedMs)}</span>
          </div>
          <button className="icon-button" type="button" title="课堂历史" onClick={openHistory}>
            <History size={19} />
          </button>
          <button
            className="icon-button"
            type="button"
            title="设置"
            onClick={() => setSettingsOpen(true)}
          >
            <Settings size={19} />
          </button>
        </div>
      </header>

      <main className="workspace">
        <section className="summary-pane" aria-labelledby="summary-heading">
          <header className="pane-header">
            <div className="left-pane-tabs" role="tablist">
              <button className={leftPaneTab === "summary" ? "active" : ""} type="button" role="tab" onClick={() => setLeftPaneTab("summary")}>
                <Sparkles size={15} />
                <span id="summary-heading">阶段总结</span>
              </button>
              <button className={leftPaneTab === "chat" ? "active" : ""} type="button" role="tab" onClick={() => setLeftPaneTab("chat")}>
                <BrainCircuit size={15} />
                <span>AI 问答</span>
              </button>
            </div>
            {leftPaneTab === "summary" && (
              <div className="summary-header-actions">
                {displayLectureNeedsSummary && (
                  <button
                    className="generate-lecture-summary-button"
                    type="button"
                    disabled={regeneratingLectureSummary}
                    onClick={() => void regenerateArchivedLectureSummary()}
                  >
                    {regeneratingLectureSummary
                      ? <LoaderCircle className="spin" size={13} />
                      : <RotateCcw size={13} />}
                    {regeneratingLectureSummary ? "生成中" : "生成整课总结"}
                  </button>
                )}
                <span className="count-badge">{displaySummaries.length}</span>
              </div>
            )}
          </header>

          {leftPaneTab === "summary" ? <div className="summary-stream" ref={summaryPaneRef} onScroll={handleSummaryScroll}>
            {!!bookmarks.length && (
              <div className="bookmark-strip" aria-label="课堂标记">
                {bookmarks.map((bookmark) => (
                  <span className="bookmark-chip" key={bookmark.id}>
                    <button type="button" title={bookmark.note || "跳到标记"} onClick={() => jumpToSegment(bookmark.segmentId)}>
                      <Bookmark size={13} />
                      {formatTimestamp(bookmark.timestampMs)}
                    </button>
                    <button className="bookmark-edit-button" type="button" title="编辑标记" onClick={() => setEditingBookmark(bookmark)}>
                      <Pencil size={11} />
                    </button>
                  </span>
                ))}
              </div>
            )}
            {displaySummaries.map((summary) => (
                <article
                  className={`summary-item${summary.kind === "lecture" ? " lecture-summary-item" : ""}`}
                  key={summary.id}
                >
                  <header>
                    <time>{formatTimestamp(summary.startMs)}</time>
                    <div className="summary-card-actions">
                      {summary.kind === "lecture" && <span className="lecture-label">整课复习</span>}
                      {summary.webEnriched && <span className="web-label">联网</span>}
                      {summary.isDemo && <span className="demo-label">DEMO</span>}
                      <button
                        className="icon-button summary-detail-button"
                        type="button"
                        title="查看完整总结"
                        onClick={() => setSelectedSummary(summary)}
                      >
                        <Maximize2 size={15} />
                      </button>
                    </div>
                  </header>
                  <h2>{summary.title}</h2>
                  {summary.kind === "lecture" && (
                    <p className="lecture-summary-coverage">
                      {summary.points.length} 条主线 · {summary.knowledgePoints?.length ?? 0} 个详细知识点
                    </p>
                  )}
                  <ul>
                    {summary.points.map((point, index) => (
                      <li key={`${summary.id}-${index}`}>{point}</li>
                    ))}
                  </ul>
                </article>
              ))}
            {displayLectureNeedsSummary && (
              <div className="missing-lecture-summary">
                <div>
                  <strong>这节课还没有整课复习资料</strong>
                  <span>阶段总结和完整转写已保存，可以直接重新生成。</span>
                </div>
                <button
                  className="primary-button"
                  type="button"
                  disabled={regeneratingLectureSummary}
                  onClick={() => void regenerateArchivedLectureSummary()}
                >
                  {regeneratingLectureSummary
                    ? <LoaderCircle className="spin" size={15} />
                    : <RotateCcw size={15} />}
                  {regeneratingLectureSummary ? "正在生成整课总结" : "生成整课总结"}
                </button>
              </div>
            )}
            {!isViewingHistory && session.isSummarizing && (
              <div className="summary-loading" role="status">
                <LoaderCircle className="spin" size={18} />
                <div>
                  <strong>
                    {session.isEnding
                      ? "正在生成整课复习资料"
                      : session.summaryStatus === "searching"
                        ? "正在联网拓展"
                        : "正在整理本段知识点"}
                  </strong>
                  <span>
                    {session.isEnding
                      ? "整合全课主线、知识点、例子与复习问题"
                      : "生成可快速回顾的阶段总结"}
                  </span>
                </div>
              </div>
            )}
            {!displaySummaries.length && (!session.isSummarizing || isViewingHistory) && (
              <div className="empty-state compact-empty">
                <ListChecks size={25} />
                <p>等待主题完成</p>
              </div>
            )}
          </div> : (
            <ChatPanel
              courseId={settings.courseId}
              lectureId={displayLectureId}
              preferences={summaryPreferences}
              onCitation={(citation) => void openCitation(citation)}
            />
          )}
          {leftPaneTab === "summary" && !summaryFollowingLatest && !isViewingHistory && (
            <button className="return-latest-button summary-return-latest-button" type="button" onClick={jumpToLatestSummary}>
              <ChevronDownCircle size={17} />
              回到最新{unreadSummaries ? ` · ${unreadSummaries} 条总结` : ""}
            </button>
          )}
        </section>

        <section className="transcript-pane" aria-labelledby="transcript-heading">
          <header className="pane-header">
            <div>
              <span className="pane-icon transcript-icon">
                <Mic size={17} />
              </span>
              <h1 id="transcript-heading">实时记录</h1>
            </div>
            <div className="transcript-meta">
              {isActive && (
                <div className="performance-status">
                  <button
                    className="performance-summary"
                    type="button"
                    aria-expanded={showPerformanceDetails}
                    aria-controls="performance-details"
                    onClick={() => setShowPerformanceDetails((current) => !current)}
                    title="查看当前课堂使用的模型和延迟"
                  >
                    <Mic size={12} />
                    <span>{shortModelName(asrModel)}</span>
                    <strong className={latencyClass(session.asrLatencyMs)}>
                      {session.asrLatencyMs === null ? "--" : `${(session.asrLatencyMs / 1_000).toFixed(1)}s`}
                    </strong>
                    <Languages size={12} />
                    <span>{shortModelName(translationModel)}</span>
                    <strong className={latencyClass(session.translationLatencyMs)}>
                      {session.translationLatencyMs === null ? "--" : `${(session.translationLatencyMs / 1_000).toFixed(1)}s`}
                    </strong>
                    <ChevronDown size={12} className={showPerformanceDetails ? "performance-chevron expanded" : "performance-chevron"} />
                  </button>
                  {showPerformanceDetails && (
                    <div className="performance-details" id="performance-details">
                      <div><Mic size={13} /><span>语音识别</span><small title={asrModel}>{asrModel}</small><b>首字 {session.asrLatencyMs === null ? "--" : `${(session.asrLatencyMs / 1_000).toFixed(1)}s`}</b></div>
                      <div><Languages size={13} /><span>实时翻译</span><small title={translationModel}>{displayModelName(translationModel)}</small><b>首字 {session.translationLatencyMs === null ? "--" : `${(session.translationLatencyMs / 1_000).toFixed(1)}s`} · 完整 {session.translationTotalMs === null ? "--" : `${(session.translationTotalMs / 1_000).toFixed(1)}s`}</b></div>
                    </div>
                  )}
                </div>
              )}
              {(slideMatch || lectureDocuments.length > 0) && (
                <button
                  className="slide-match-button"
                  type="button"
                  title={slideMatch
                    ? `${slideMatch.heading} · 匹配置信度 ${Math.round(slideMatch.confidence * 100)}%`
                    : "浏览 Slides 并设置当前页"}
                  onClick={() => void openMatchedSlide()}
                >
                  <FileText size={13} />
                  {slideMatch ? `Slides P.${slideMatch.pageNumber}` : "Slides"}
                </button>
              )}
              <span className="language-pair">EN · 中文</span>
              {isViewingHistory && (
                <button
                  className="history-return-button"
                  type="button"
                  onClick={() => void returnToCurrentLecture()}
                >
                  <ChevronLeft size={13} />
                  返回当前课堂
                </button>
              )}
            </div>
          </header>

          <div className="transcript-stream" ref={transcriptPaneRef} onScroll={handleTranscriptScroll}>
            {displaySegments.length ? (
              displaySegments.map((segment) => (
                <TranscriptRow
                  segment={segment}
                  key={segment.id}
                  bookmarked={bookmarks.some((bookmark) => bookmark.segmentId === segment.id)}
                  highlighted={highlightedSegmentId === segment.id}
                  onRetry={isViewingHistory ? () => { setTranslationSegment(segment); setTranslationError(null); } : undefined}
                />
              ))
            ) : (
              <div className="empty-state">
                <div className="empty-mic">
                  <Mic size={26} />
                </div>
                <p>等待课堂开始</p>
                {!isActive && !session.hasRecoveredLecture && (
                  <div className="empty-actions">
                    <button className="primary-button" type="button" onClick={beginLecture}>
                      <Mic size={17} />
                      开始听课
                    </button>
                    <button className="secondary-button" type="button" onClick={session.startDemo}>
                      <Play size={17} />
                      演示
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>
          <TranslationDialog segment={translationSegment} busy={translationBusy} error={translationError} onTranslate={() => void retryArchivedTranslation()} onClose={() => { if (!translationBusy) setTranslationSegment(null); }} />
          {!followingLatest && !isViewingHistory && (
            <button className="return-latest-button" type="button" onClick={jumpToLatest}>
              <ChevronDownCircle size={17} />
              回到最新{unreadSegments ? ` · ${unreadSegments} 条` : ""}
            </button>
          )}
        </section>
      </main>

      <footer className="controlbar">
        <div className="audio-source">
          <span className={`mic-status ${isActive ? "active" : ""}`}>
            <Mic size={17} />
          </span>
          <div>
            <strong>{session.status === "demo" ? "演示音频" : "系统麦克风"}</strong>
            <span>{statusLabels[session.status]}</span>
          </div>
          <div className="level-meter" aria-label="麦克风音量">
            {Array.from({ length: 8 }).map((_, index) => (
              <i
                key={index}
                className={index / 8 < session.audioLevel ? "lit" : ""}
                style={{ height: `${7 + index * 2}px` }}
              />
            ))}
          </div>
        </div>

        <div className="secondary-controls">
          {isViewingHistory && displayLectureId !== null && (
            <button className="tool-button" type="button" onClick={() => void exportDisplayedLecture()}>
              <Download size={17} />
              导出课堂
            </button>
          )}
          <button
            className="tool-button"
            type="button"
            title="添加标记"
            disabled={!isActive || session.lectureId === null}
            onClick={() => void addBookmark()}
          >
            <Bookmark size={17} />
            添加标记
          </button>
          <button
            className="tool-button"
            type="button"
            onClick={session.addManualSummary}
            disabled={
              session.isSummarizing ||
              !session.segments.length ||
              isViewingHistory ||
              !["live", "paused", "demo"].includes(session.status)
            }
          >
            {session.isSummarizing ? <LoaderCircle className="spin" size={17} /> : <Sparkles size={17} />}
            {session.isSummarizing ? "总结中" : "立即总结"}
          </button>
        </div>

        <div className="primary-controls">
          {!isActive ? (
            <>
              {!session.hasRecoveredLecture && (
                <button className="secondary-button" type="button" onClick={session.startDemo}>
                  <Play size={17} />
                  演示
                </button>
              )}
              <button className="record-button" type="button" onClick={beginLecture}>
                <Mic size={18} />
                {session.hasRecoveredLecture || session.status === "error" ? "继续听课" : "开始听课"}
              </button>
            </>
          ) : (
            <>
              {canPause && (
                <button className="secondary-button" type="button" onClick={session.togglePause}>
                  {session.status === "paused" ? <Play size={17} /> : <Pause size={17} />}
                  {session.status === "paused" ? "继续" : "暂停"}
                </button>
              )}
              <button
                className="stop-button"
                type="button"
                disabled={session.isEnding || (session.isSummarizing && !session.isEnding)}
                onClick={() => void session.stop()}
              >
                {session.isEnding ? <LoaderCircle className="spin" size={18} /> : <CircleStop size={18} />}
                {session.isEnding ? "整课总结中" : "结束"}
              </button>
            </>
          )}
        </div>
      </footer>

      {session.hasRecoveredLecture && (
        <div className="recovery-banner" role="status">
          <div>
            <strong>已恢复上次未结束的课堂</strong>
            <span>{session.segments.length} 个语段已从本机数据库载入</span>
          </div>
          <button className="secondary-button" type="button" onClick={() => void session.discardRecovered()}>
            结束此记录
          </button>
          <button className="primary-button" type="button" onClick={beginLecture}>
            <Play size={16} />
            继续听课
          </button>
        </div>
      )}

      {(session.error || appError) && (
        <div className="error-toast" role="alert">
          <span>{session.error || appError}</span>
          <button
            className="icon-button"
            type="button"
            onClick={() => {
              session.clearError();
              setAppError(null);
            }}
            title="关闭"
          >
            <X size={17} />
          </button>
        </div>
      )}

      <CourseDialog
        open={settingsOpen}
        courses={courses}
        initialCourse={currentCourse}
        initialTerms={terms}
        initialSummaryPreferences={summaryPreferences}
        hasOpenAiApiKey={hasOpenAiApiKey}
        hasAlibabaApiKey={hasAlibabaApiKey}
        hasAlibabaAsrApiKey={hasAlibabaAsrApiKey}
        hasDeepgramApiKey={hasDeepgramApiKey}
        courseLocked={courseLocked}
        onClose={() => setSettingsOpen(false)}
        onLoadCourse={fetchCourse}
        onSave={saveCourse}
        onDeleteKey={deleteApiKey}
      />

      <LectureSetupDialog
        open={lectureSetupOpen}
        course={currentCourse}
        starting={startingLecture}
        onClose={() => setLectureSetupOpen(false)}
        onStart={startLectureFromSetup}
      />

      <HistoryDialog
        open={historyOpen}
        course={currentCourse}
        lectures={lectures}
        loading={historyLoading}
        onClose={() => setHistoryOpen(false)}
        onOpenLecture={(lecture) => void openArchivedLecture(lecture)}
        onDeleteLecture={setLecturePendingDelete}
        onBackup={backupAllData}
        onRestore={restoreBackup}
        onBackfillSummaries={backfillCourseLectureSummaries}
        backfillProgress={backfillProgress}
      />

      <DeleteLectureDialog
        lecture={lecturePendingDelete}
        deleting={deletingLecture}
        onClose={() => setLecturePendingDelete(null)}
        onConfirm={deletePendingLecture}
      />

      <SummaryDetailDialog
        summary={selectedSummary}
        onClose={() => setSelectedSummary(null)}
        onRegenerate={isViewingHistory && selectedSummary?.kind === "lecture"
          ? () => void regenerateArchivedLectureSummary()
          : undefined}
        regenerating={regeneratingLectureSummary}
      />

      <SlideDetailDialog
        chunk={selectedDocumentChunk}
        pageCount={selectedDocumentChunks.length}
        hasPrevious={selectedDocumentChunk ? selectedDocumentChunks.findIndex((chunk) => chunk.id === selectedDocumentChunk.id) > 0 : false}
        hasNext={selectedDocumentChunk ? selectedDocumentChunks.findIndex((chunk) => chunk.id === selectedDocumentChunk.id) < selectedDocumentChunks.length - 1 : false}
        isCurrent={Boolean(selectedDocumentChunk && slideMatch?.documentId === selectedDocumentChunk.documentId && slideMatch.pageNumber === selectedDocumentChunk.pageNumber)}
        onNavigate={navigateSelectedSlide}
        onJump={jumpToSelectedSlide}
        onSetCurrent={setSelectedSlideAsCurrent}
        onClose={() => {
          setSelectedDocumentChunk(null);
          setSelectedDocumentChunks([]);
        }}
      />

      <BookmarkDialog
        bookmark={editingBookmark}
        onClose={() => setEditingBookmark(null)}
        onSave={saveBookmarkNote}
        onDelete={deleteEditingBookmark}
      />

      {!hasOpenAiApiKey && !isTauri() && (
        <div className="browser-badge">
          <RotateCcw size={14} />
          浏览器预览
        </div>
      )}
    </div>
  );
}
