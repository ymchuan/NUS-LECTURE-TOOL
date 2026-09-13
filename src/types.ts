export type SegmentState = "interim" | "translating" | "complete" | "error";

export interface TranscriptSegment {
  id: string;
  itemId: string;
  startMs: number;
  english: string;
  chinese: string;
  state: SegmentState;
}

export interface TopicSummary {
  id: string;
  kind?: "topic" | "lecture";
  startMs: number;
  endMs?: number;
  title: string;
  points: string[];
  overview?: string;
  knowledgePoints?: KnowledgePoint[];
  definitions?: DefinitionItem[];
  examples?: string[];
  examTips?: string[];
  questions?: string[];
  mindMap?: MindMap;
  webInsights?: WebInsight[];
  sources?: SummarySource[];
  webEnriched?: boolean;
  model?: string;
  isDemo?: boolean;
}

export interface KnowledgePoint {
  title: string;
  explanation: string;
  lecturerEvidence: string;
  importance: "core" | "supporting";
}

export interface DefinitionItem {
  term: string;
  definition: string;
}

export interface MindMapLeaf {
  label: string;
  note: string;
}

export interface MindMapBranch {
  label: string;
  children: MindMapLeaf[];
}

export interface MindMap {
  root: string;
  branches: MindMapBranch[];
}

export interface WebInsight {
  title: string;
  summary: string;
  explanation?: string;
  keyPoints?: string[];
  formulas?: InsightFormula[];
  howToUse?: string[];
  sources?: SummarySource[];
}

export interface InsightFormula {
  name: string;
  expression: string;
  variables: string[];
  useWhen: string;
  steps: string[];
  workedExample: string;
}

export interface SummarySource {
  title: string;
  url: string;
}

export type ModelProvider = "openai" | "alibaba";
export type TranslationProvider = ModelProvider | "local";
export type TranscriptionProvider = "openai" | "alibaba" | "deepgram";

export interface SummaryPreferences {
  asrConfigVersion: number;
  textConfigVersion: number;
  autoSummaryEnabled: boolean;
  webSearchEnabled: boolean;
  transcriptionProvider: TranscriptionProvider;
  transcriptionModel: string;
  microphoneDeviceId: string;
  textProvider: ModelProvider;
  translationProvider: TranslationProvider;
  alibabaWorkspaceId: string;
  translationModel: string;
  localTranslationModel: string;
  localTranslationEndpoint: string;
  openaiBaseUrl?: string;
  summaryModel: string;
  lectureSummaryModel: string;
}

export interface Course {
  id: number;
  code: string;
  name: string;
  description: string;
  createdAt: number;
}

export interface GlossaryTerm {
  id?: number;
  courseId?: number;
  english: string;
  chinese: string;
  aliases: string;
  priority: number;
  enabled: boolean;
}

export interface AsrHotword {
  text: string;
  weight: number;
}

export interface CourseSettings {
  courseId: number | null;
  courseCode: string;
  courseName: string;
  courseContext: string;
  keywords: string[];
  hotwords: AsrHotword[];
  glossary: string[];
  transcriptionModel: string;
  translationModel: string;
  summaryModel: string;
}

export type SessionStatus =
  | "idle"
  | "connecting"
  | "live"
  | "paused"
  | "recovered"
  | "demo"
  | "error";

export interface TranslationEvent {
  segmentId: string;
  kind: "delta" | "done" | "error";
  text: string;
}

export interface StreamingAsrEvent {
  kind: "delta" | "final" | "error";
  itemId: string;
  text: string;
  startMs: number;
  endMs?: number;
}

export type AlibabaAsrEvent = StreamingAsrEvent;

export interface LectureSnapshot {
  lectureId: number;
  courseId: number;
  title: string;
  startedAt: number;
  elapsedMs: number;
  status: string;
  segments: TranscriptSegment[];
  summaries: TopicSummary[];
}

export interface LectureListItem {
  id: number;
  courseId: number;
  title: string;
  startedAt: number;
  endedAt?: number;
  elapsedMs: number;
  status: string;
  segmentCount: number;
  summaryCount: number;
  bookmarkCount: number;
  documentCount: number;
}

export interface LectureBookmark {
  id: number;
  lectureId: number;
  segmentId?: string;
  timestampMs: number;
  note: string;
  createdAt: number;
}

export interface LectureDocument {
  id: number;
  courseId: number;
  lectureId?: number;
  kind: "slides";
  name: string;
  mimeType: string;
  pageCount: number;
  parseStatus: string;
  createdAt: number;
}

export interface DocumentChunk {
  id: number;
  documentId: number;
  pageNumber: number;
  heading: string;
  content: string;
}

export interface DocumentSource {
  documentId: number;
  name: string;
  mimeType: string;
  pageCount: number;
  dataBase64: string;
}

export interface SlideMatch {
  documentId: number;
  documentName: string;
  pageNumber: number;
  heading: string;
  excerpt: string;
  score: number;
  confidence: number;
}

export type ChatScope = "lecture" | "course";
export type CitationKind = "transcript" | "slide" | "syllabus" | "web";

export interface ChatCitation {
  id: string;
  kind: CitationKind;
  title: string;
  excerpt: string;
  lectureId?: number;
  segmentId?: string;
  timestampMs?: number;
  documentId?: number;
  pageNumber?: number;
  url?: string;
}

export interface ChatMessage {
  id: number;
  threadId: number;
  role: "user" | "assistant";
  content: string;
  citations: ChatCitation[];
  model?: string;
  createdAt: number;
}

export interface ChatThread {
  id: number;
  courseId: number;
  lectureId?: number;
  scope: ChatScope;
  title: string;
  createdAt: number;
  updatedAt: number;
}

export interface ChatAnswer {
  thread: ChatThread;
  message: ChatMessage;
}
