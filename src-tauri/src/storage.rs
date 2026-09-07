use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::State;

pub struct Database(pub Mutex<Connection>);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseInput {
    pub id: Option<i64>,
    pub code: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Course {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub description: String,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlossaryTermInput {
    pub english: String,
    pub chinese: String,
    pub aliases: String,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlossaryTerm {
    pub id: i64,
    pub course_id: i64,
    pub english: String,
    pub chinese: String,
    pub aliases: String,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginLectureInput {
    pub course_id: i64,
    pub title: String,
    pub started_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegmentInput {
    pub id: String,
    pub start_ms: i64,
    pub english: String,
    pub chinese: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTranscriptSegment {
    pub id: String,
    pub item_id: String,
    pub start_ms: i64,
    pub english: String,
    pub chinese: String,
    pub state: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePoint {
    pub title: String,
    pub explanation: String,
    pub lecturer_evidence: String,
    pub importance: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionItem {
    pub term: String,
    pub definition: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MindMapLeaf {
    pub label: String,
    pub note: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MindMapBranch {
    pub label: String,
    pub children: Vec<MindMapLeaf>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MindMap {
    pub root: String,
    pub branches: Vec<MindMapBranch>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SummarySource {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct InsightFormula {
    pub name: String,
    pub expression: String,
    pub variables: Vec<String>,
    pub use_when: String,
    pub steps: Vec<String>,
    pub worked_example: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WebInsight {
    pub title: String,
    pub summary: String,
    pub explanation: String,
    pub key_points: Vec<String>,
    pub formulas: Vec<InsightFormula>,
    pub how_to_use: Vec<String>,
    pub sources: Vec<SummarySource>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicSummaryInput {
    pub id: String,
    #[serde(default)]
    pub kind: Option<String>,
    pub start_ms: i64,
    #[serde(default)]
    pub end_ms: Option<i64>,
    pub title: String,
    #[serde(default)]
    pub points: Vec<String>,
    #[serde(default)]
    pub overview: String,
    #[serde(default)]
    pub knowledge_points: Vec<KnowledgePoint>,
    #[serde(default)]
    pub definitions: Vec<DefinitionItem>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub exam_tips: Vec<String>,
    #[serde(default)]
    pub questions: Vec<String>,
    #[serde(default)]
    pub mind_map: Option<MindMap>,
    #[serde(default)]
    pub web_insights: Vec<WebInsight>,
    #[serde(default)]
    pub sources: Vec<SummarySource>,
    #[serde(default)]
    pub web_enriched: bool,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub is_demo: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LectureSnapshotInput {
    pub lecture_id: i64,
    pub elapsed_ms: i64,
    pub status: String,
    pub segments: Vec<TranscriptSegmentInput>,
    pub summaries: Vec<TopicSummaryInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LectureSnapshot {
    pub lecture_id: i64,
    pub course_id: i64,
    pub title: String,
    pub started_at: i64,
    pub elapsed_ms: i64,
    pub status: String,
    pub segments: Vec<StoredTranscriptSegment>,
    pub summaries: Vec<TopicSummaryInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LectureListItem {
    pub id: i64,
    pub course_id: i64,
    pub title: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub elapsed_ms: i64,
    pub status: String,
    pub segment_count: i64,
    pub summary_count: i64,
    pub bookmark_count: i64,
    pub document_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LectureBookmark {
    pub id: i64,
    pub lecture_id: i64,
    pub segment_id: Option<String>,
    pub timestamp_ms: i64,
    pub note: String,
    pub created_at: i64,
}

impl Database {
    pub fn open(app_data_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(app_data_dir)
            .map_err(|error| format!("无法创建应用数据目录：{error}"))?;
        let path = app_data_dir.join("lecture-assistant.sqlite3");
        let connection =
            Connection::open(path).map_err(|error| format!("无法打开课堂数据库：{error}"))?;
        initialize(&connection)?;
        Ok(Self(Mutex::new(connection)))
    }

    #[cfg(test)]
    fn in_memory() -> Result<Self, String> {
        let connection =
            Connection::open_in_memory().map_err(|error| format!("无法创建测试数据库：{error}"))?;
        initialize(&connection)?;
        Ok(Self(Mutex::new(connection)))
    }
}

fn initialize(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;

            CREATE TABLE IF NOT EXISTS courses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                code TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS glossary_terms (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
                english TEXT NOT NULL,
                chinese TEXT NOT NULL DEFAULT '',
                aliases TEXT NOT NULL DEFAULT '',
                priority INTEGER NOT NULL DEFAULT 2,
                enabled INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS lectures (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
                title TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                ended_at INTEGER,
                status TEXT NOT NULL,
                elapsed_ms INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS transcript_segments (
                id TEXT NOT NULL,
                lecture_id INTEGER NOT NULL REFERENCES lectures(id) ON DELETE CASCADE,
                seq INTEGER NOT NULL,
                start_ms INTEGER NOT NULL,
                end_ms INTEGER,
                english TEXT NOT NULL,
                chinese TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                PRIMARY KEY (lecture_id, id)
            );

            CREATE TABLE IF NOT EXISTS topic_summaries (
                id TEXT NOT NULL,
                lecture_id INTEGER NOT NULL REFERENCES lectures(id) ON DELETE CASCADE,
                start_ms INTEGER NOT NULL,
                end_ms INTEGER,
                title TEXT NOT NULL,
                content_json TEXT NOT NULL,
                PRIMARY KEY (lecture_id, id)
            );

            CREATE TABLE IF NOT EXISTS lecture_summaries (
                lecture_id INTEGER PRIMARY KEY REFERENCES lectures(id) ON DELETE CASCADE,
                content_json TEXT NOT NULL,
                model TEXT NOT NULL,
                generated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS lecture_bookmarks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                lecture_id INTEGER NOT NULL REFERENCES lectures(id) ON DELETE CASCADE,
                segment_id TEXT,
                timestamp_ms INTEGER NOT NULL,
                note TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS course_documents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
                lecture_id INTEGER REFERENCES lectures(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                stored_path TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                page_count INTEGER NOT NULL DEFAULT 0,
                parse_status TEXT NOT NULL DEFAULT 'ready',
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS document_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id INTEGER NOT NULL REFERENCES course_documents(id) ON DELETE CASCADE,
                page_number INTEGER NOT NULL,
                heading TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS chat_threads (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
                lecture_id INTEGER REFERENCES lectures(id) ON DELETE CASCADE,
                scope TEXT NOT NULL,
                title TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                thread_id INTEGER NOT NULL REFERENCES chat_threads(id) ON DELETE CASCADE,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                citations_json TEXT NOT NULL DEFAULT '[]',
                model TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_lectures_course_started
                ON lectures(course_id, started_at DESC);
            CREATE INDEX IF NOT EXISTS idx_bookmarks_lecture_time
                ON lecture_bookmarks(lecture_id, timestamp_ms);
            CREATE INDEX IF NOT EXISTS idx_documents_lecture
                ON course_documents(lecture_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_chunks_document_page
                ON document_chunks(document_id, page_number);
            CREATE INDEX IF NOT EXISTS idx_chat_threads_course
                ON chat_threads(course_id, lecture_id, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_chat_messages_thread
                ON chat_messages(thread_id, created_at);

            DELETE FROM chat_messages
                WHERE NOT EXISTS (SELECT 1 FROM chat_threads t WHERE t.id = chat_messages.thread_id);
            DELETE FROM document_chunks
                WHERE NOT EXISTS (SELECT 1 FROM course_documents d WHERE d.id = document_chunks.document_id);
            DELETE FROM transcript_segments
                WHERE NOT EXISTS (SELECT 1 FROM lectures l WHERE l.id = transcript_segments.lecture_id);
            DELETE FROM transcript_segments
                WHERE status = 'interim'
                  AND EXISTS (
                      SELECT 1 FROM lectures l
                      WHERE l.id = transcript_segments.lecture_id AND l.status = 'ended'
                  );
            UPDATE transcript_segments
                SET status = 'error'
                WHERE status = 'translating'
                  AND EXISTS (
                      SELECT 1 FROM lectures l
                      WHERE l.id = transcript_segments.lecture_id AND l.status = 'ended'
                  );
            DELETE FROM topic_summaries
                WHERE NOT EXISTS (SELECT 1 FROM lectures l WHERE l.id = topic_summaries.lecture_id);
            DELETE FROM lecture_summaries
                WHERE NOT EXISTS (SELECT 1 FROM lectures l WHERE l.id = lecture_summaries.lecture_id);
            DELETE FROM lecture_bookmarks
                WHERE NOT EXISTS (SELECT 1 FROM lectures l WHERE l.id = lecture_bookmarks.lecture_id);
            DELETE FROM course_documents
                WHERE NOT EXISTS (SELECT 1 FROM courses c WHERE c.id = course_documents.course_id)
                   OR (lecture_id IS NOT NULL AND NOT EXISTS (
                       SELECT 1 FROM lectures l WHERE l.id = course_documents.lecture_id
                   ));
            DELETE FROM chat_threads
                WHERE NOT EXISTS (SELECT 1 FROM courses c WHERE c.id = chat_threads.course_id)
                   OR (lecture_id IS NOT NULL AND NOT EXISTS (
                       SELECT 1 FROM lectures l WHERE l.id = chat_threads.lecture_id
                   ));
            DELETE FROM glossary_terms
                WHERE NOT EXISTS (SELECT 1 FROM courses c WHERE c.id = glossary_terms.course_id);
            DELETE FROM lectures
                WHERE NOT EXISTS (SELECT 1 FROM courses c WHERE c.id = lectures.course_id);
            "#,
        )
        .map_err(|error| format!("无法初始化课堂数据库：{error}"))
}

fn lock_database<'a>(
    database: &'a State<'_, Database>,
) -> Result<std::sync::MutexGuard<'a, Connection>, String> {
    database
        .0
        .lock()
        .map_err(|_| "课堂数据库锁已损坏".to_string())
}

fn row_to_course(row: &rusqlite::Row<'_>) -> rusqlite::Result<Course> {
    Ok(Course {
        id: row.get(0)?,
        code: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        created_at: row.get(4)?,
    })
}

#[tauri::command]
pub fn list_courses(database: State<'_, Database>) -> Result<Vec<Course>, String> {
    let connection = lock_database(&database)?;
    let mut statement = connection
        .prepare(
            "SELECT id, code, name, description, created_at FROM courses ORDER BY created_at, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], row_to_course)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_course(database: State<'_, Database>, course: CourseInput) -> Result<Course, String> {
    let name = course.name.trim();
    if name.is_empty() {
        return Err("课程名称不能为空".to_string());
    }

    let connection = lock_database(&database)?;
    let id =
        if let Some(id) = course.id {
            let changed = connection
                .execute(
                    "UPDATE courses SET code = ?1, name = ?2, description = ?3 WHERE id = ?4",
                    params![course.code.trim(), name, course.description.trim(), id],
                )
                .map_err(|error| error.to_string())?;
            if changed == 0 {
                return Err("没有找到要更新的课程".to_string());
            }
            id
        } else {
            let created_at = chrono_timestamp_ms();
            connection
            .execute(
                "INSERT INTO courses (code, name, description, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![course.code.trim(), name, course.description.trim(), created_at],
            )
            .map_err(|error| error.to_string())?;
            connection.last_insert_rowid()
        };

    connection
        .query_row(
            "SELECT id, code, name, description, created_at FROM courses WHERE id = ?1",
            [id],
            row_to_course,
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_glossary_terms(
    database: State<'_, Database>,
    course_id: i64,
) -> Result<Vec<GlossaryTerm>, String> {
    let connection = lock_database(&database)?;
    list_glossary_terms_inner(&connection, course_id)
}

fn list_glossary_terms_inner(
    connection: &Connection,
    course_id: i64,
) -> Result<Vec<GlossaryTerm>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, course_id, english, chinese, aliases, priority, enabled
             FROM glossary_terms WHERE course_id = ?1 ORDER BY priority DESC, english COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([course_id], |row| {
            Ok(GlossaryTerm {
                id: row.get(0)?,
                course_id: row.get(1)?,
                english: row.get(2)?,
                chinese: row.get(3)?,
                aliases: row.get(4)?,
                priority: row.get(5)?,
                enabled: row.get::<_, i64>(6)? != 0,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn replace_glossary_terms(
    database: State<'_, Database>,
    course_id: i64,
    terms: Vec<GlossaryTermInput>,
) -> Result<Vec<GlossaryTerm>, String> {
    let mut connection = lock_database(&database)?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM glossary_terms WHERE course_id = ?1",
            [course_id],
        )
        .map_err(|error| error.to_string())?;

    for term in terms {
        let english = term.english.trim();
        if english.is_empty() {
            continue;
        }
        transaction
            .execute(
                "INSERT INTO glossary_terms (course_id, english, chinese, aliases, priority, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    course_id,
                    english,
                    term.chinese.trim(),
                    term.aliases.trim(),
                    term.priority.clamp(1, 3),
                    i64::from(term.enabled)
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    list_glossary_terms_inner(&connection, course_id)
}

#[tauri::command]
pub fn begin_lecture(
    database: State<'_, Database>,
    lecture: BeginLectureInput,
) -> Result<i64, String> {
    let title = lecture.title.trim();
    if title.is_empty() {
        return Err("课堂标题不能为空".to_string());
    }
    let connection = lock_database(&database)?;
    connection
        .execute(
            "INSERT INTO lectures (course_id, title, started_at, status, elapsed_ms)
             VALUES (?1, ?2, ?3, 'connecting', 0)",
            params![lecture.course_id, title, lecture.started_at],
        )
        .map_err(|error| error.to_string())?;
    Ok(connection.last_insert_rowid())
}

#[tauri::command]
pub fn save_lecture_snapshot(
    database: State<'_, Database>,
    snapshot: LectureSnapshotInput,
) -> Result<(), String> {
    let mut connection = lock_database(&database)?;
    save_lecture_snapshot_inner(&mut connection, snapshot)
}

#[tauri::command]
pub fn save_transcript_translation(
    database: State<'_, Database>,
    lecture_id: i64,
    segment_id: String,
    chinese: String,
    state: String,
) -> Result<(), String> {
    let mut connection = lock_database(&database)?;
    save_transcript_translation_inner(&mut connection, lecture_id, &segment_id, &chinese, &state)
}

fn save_transcript_translation_inner(
    connection: &mut Connection,
    lecture_id: i64,
    segment_id: &str,
    chinese: &str,
    state: &str,
) -> Result<(), String> {
    if segment_id.trim().is_empty() || segment_id.chars().count() > 200 {
        return Err("语段 ID 不能为空且不能超过 200 个字符".to_string());
    }
    if !matches!(state, "complete" | "error") {
        return Err("翻译状态只能是 complete 或 error".to_string());
    }
    if chinese.chars().count() > 32_000 {
        return Err("译文不能超过 32000 个字符".to_string());
    }
    if state == "complete" && chinese.trim().is_empty() {
        return Err("翻译完成时译文不能为空".to_string());
    }

    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let lecture_exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM lectures WHERE id = ?1)",
            [lecture_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !lecture_exists {
        return Err("没有找到要更新翻译的课堂记录".to_string());
    }
    let current_state: Option<String> = transaction
        .query_row(
            "SELECT status FROM transcript_segments WHERE lecture_id = ?1 AND id = ?2",
            params![lecture_id, segment_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    match current_state.as_deref() {
        None => return Err("没有找到要更新翻译的语段".to_string()),
        Some("interim") => return Err("不能保存尚未定稿语段的翻译".to_string()),
        _ => {}
    }
    let changed = transaction
        .execute(
            "UPDATE transcript_segments SET chinese = ?1, status = ?2
             WHERE lecture_id = ?3 AND id = ?4 AND status != 'interim'",
            params![chinese, state, lecture_id, segment_id],
        )
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("没有找到可更新翻译的语段".to_string());
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn save_lecture_snapshot_inner(
    connection: &mut Connection,
    snapshot: LectureSnapshotInput,
) -> Result<(), String> {
    let active_interim_id = snapshot
        .segments
        .iter()
        .rev()
        .find(|segment| segment.state == "interim")
        .map(|segment| segment.id.clone());
    let summary_ids = snapshot
        .summaries
        .iter()
        .map(|summary| summary.id.clone())
        .collect::<HashSet<_>>();
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let changed = transaction
        .execute(
            "UPDATE lectures SET elapsed_ms = ?1, status = ?2 WHERE id = ?3",
            params![snapshot.elapsed_ms, snapshot.status, snapshot.lecture_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("没有找到要保存的课堂记录".to_string());
    }

    for (sequence, segment) in snapshot.segments.into_iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO transcript_segments
                 (id, lecture_id, seq, start_ms, english, chinese, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(lecture_id, id) DO UPDATE SET
                   seq = excluded.seq,
                   start_ms = excluded.start_ms,
                   english = excluded.english,
                   chinese = excluded.chinese,
                   status = excluded.status",
                params![
                    segment.id,
                    snapshot.lecture_id,
                    sequence as i64,
                    segment.start_ms,
                    segment.english,
                    segment.chinese,
                    segment.state
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    if let Some(active_interim_id) = active_interim_id {
        transaction
            .execute(
                "DELETE FROM transcript_segments
                 WHERE lecture_id = ?1 AND status = 'interim' AND id != ?2",
                params![snapshot.lecture_id, active_interim_id],
            )
            .map_err(|error| error.to_string())?;
    } else {
        transaction
            .execute(
                "DELETE FROM transcript_segments WHERE lecture_id = ?1 AND status = 'interim'",
                [snapshot.lecture_id],
            )
            .map_err(|error| error.to_string())?;
    }

    for summary in snapshot.summaries {
        let content_json =
            serde_json::to_string(&json_summary(&summary)).map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO topic_summaries (id, lecture_id, start_ms, title, content_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(lecture_id, id) DO UPDATE SET
                   start_ms = excluded.start_ms,
                   title = excluded.title,
                   content_json = excluded.content_json",
                params![
                    summary.id,
                    snapshot.lecture_id,
                    summary.start_ms,
                    summary.title,
                    content_json
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    let stored_summary_ids = {
        let mut statement = transaction
            .prepare("SELECT id FROM topic_summaries WHERE lecture_id = ?1")
            .map_err(|error| error.to_string())?;
        let ids = statement
            .query_map([snapshot.lecture_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        ids
    };
    for stale_id in stored_summary_ids
        .into_iter()
        .filter(|id| !summary_ids.contains(id))
    {
        transaction
            .execute(
                "DELETE FROM topic_summaries WHERE lecture_id = ?1 AND id = ?2",
                params![snapshot.lecture_id, stale_id],
            )
            .map_err(|error| error.to_string())?;
    }

    transaction.commit().map_err(|error| error.to_string())
}

fn json_summary(summary: &TopicSummaryInput) -> serde_json::Value {
    serde_json::json!({
        "kind": summary.kind,
        "endMs": summary.end_ms,
        "points": summary.points,
        "overview": summary.overview,
        "knowledgePoints": summary.knowledge_points,
        "definitions": summary.definitions,
        "examples": summary.examples,
        "examTips": summary.exam_tips,
        "questions": summary.questions,
        "mindMap": summary.mind_map,
        "webInsights": summary.web_insights,
        "sources": summary.sources,
        "webEnriched": summary.web_enriched,
        "model": summary.model,
        "isDemo": summary.is_demo
    })
}

#[tauri::command]
pub fn finish_lecture(
    database: State<'_, Database>,
    lecture_id: i64,
    elapsed_ms: i64,
    ended_at: i64,
) -> Result<(), String> {
    let mut connection = lock_database(&database)?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let changed = transaction
        .execute(
            "UPDATE lectures SET ended_at = ?1, elapsed_ms = ?2, status = 'ended' WHERE id = ?3",
            params![ended_at, elapsed_ms, lecture_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("没有找到要结束的课堂记录".to_string());
    }
    transaction
        .execute(
            "DELETE FROM transcript_segments WHERE lecture_id = ?1 AND status = 'interim'",
            [lecture_id],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

type LectureRow = (i64, i64, String, i64, i64, String);

fn load_lecture_snapshot(
    connection: &Connection,
    lecture: LectureRow,
) -> Result<LectureSnapshot, String> {
    let (lecture_id, course_id, title, started_at, elapsed_ms, status) = lecture;
    let mut segment_statement = connection
        .prepare(
            "SELECT id, start_ms, english, chinese, status
             FROM transcript_segments WHERE lecture_id = ?1 ORDER BY seq",
        )
        .map_err(|error| error.to_string())?;
    let segments = segment_statement
        .query_map([lecture_id], |row| {
            let id: String = row.get(0)?;
            Ok(StoredTranscriptSegment {
                item_id: id.clone(),
                id,
                start_ms: row.get(1)?,
                english: row.get(2)?,
                chinese: row.get(3)?,
                state: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    let mut summary_statement = connection
        .prepare(
            "SELECT id, start_ms, title, content_json
             FROM topic_summaries WHERE lecture_id = ?1 ORDER BY start_ms",
        )
        .map_err(|error| error.to_string())?;
    let mut summaries = summary_statement
        .query_map([lecture_id], |row| {
            let id: String = row.get(0)?;
            let start_ms: i64 = row.get(1)?;
            let title: String = row.get(2)?;
            let content: String = row.get(3)?;
            let mut value: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
            if !value.is_object() {
                value = serde_json::json!({});
            }
            value["id"] = serde_json::Value::String(id.clone());
            value["startMs"] = serde_json::Value::Number(start_ms.into());
            value["title"] = serde_json::Value::String(title.clone());
            Ok(
                serde_json::from_value::<TopicSummaryInput>(value).unwrap_or(TopicSummaryInput {
                    id,
                    kind: None,
                    start_ms,
                    end_ms: None,
                    title,
                    points: vec![],
                    overview: String::new(),
                    knowledge_points: vec![],
                    definitions: vec![],
                    examples: vec![],
                    exam_tips: vec![],
                    questions: vec![],
                    mind_map: None,
                    web_insights: vec![],
                    sources: vec![],
                    web_enriched: false,
                    model: None,
                    is_demo: None,
                }),
            )
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    summaries.sort_by_key(|summary| (summary.kind.as_deref() == Some("lecture"), summary.start_ms));

    Ok(LectureSnapshot {
        lecture_id,
        course_id,
        title,
        started_at,
        elapsed_ms,
        status,
        segments,
        summaries,
    })
}

fn read_lecture_row(connection: &Connection, lecture_id: i64) -> Result<LectureRow, String> {
    connection
        .query_row(
            "SELECT id, course_id, title, started_at, elapsed_ms, status
             FROM lectures WHERE id = ?1",
            [lecture_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_lectures(
    database: State<'_, Database>,
    course_id: i64,
) -> Result<Vec<LectureListItem>, String> {
    let connection = lock_database(&database)?;
    let mut statement = connection
        .prepare(
            "SELECT l.id, l.course_id, l.title, l.started_at, l.ended_at, l.elapsed_ms, l.status,
                    (SELECT COUNT(*) FROM transcript_segments t WHERE t.lecture_id = l.id),
                    (SELECT COUNT(*) FROM topic_summaries s WHERE s.lecture_id = l.id),
                    (SELECT COUNT(*) FROM lecture_bookmarks b WHERE b.lecture_id = l.id),
                    (SELECT COUNT(*) FROM course_documents d WHERE d.lecture_id = l.id)
             FROM lectures l WHERE l.course_id = ?1 ORDER BY l.started_at DESC, l.id DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([course_id], |row| {
            Ok(LectureListItem {
                id: row.get(0)?,
                course_id: row.get(1)?,
                title: row.get(2)?,
                started_at: row.get(3)?,
                ended_at: row.get(4)?,
                elapsed_ms: row.get(5)?,
                status: row.get(6)?,
                segment_count: row.get(7)?,
                summary_count: row.get(8)?,
                bookmark_count: row.get(9)?,
                document_count: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn delete_lecture_inner(
    connection: &mut Connection,
    lecture_id: i64,
) -> Result<(String, Vec<PathBuf>), String> {
    let lecture: Option<(String, String)> = connection
        .query_row(
            "SELECT title, status FROM lectures WHERE id = ?1",
            [lecture_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((title, status)) = lecture else {
        return Err("找不到这条课堂记录".to_string());
    };
    if status != "ended" {
        return Err("只能删除已经结束的课堂记录".to_string());
    }

    let paths = {
        let mut statement = connection
            .prepare("SELECT stored_path FROM course_documents WHERE lecture_id = ?1")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([lecture_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows.into_iter().map(PathBuf::from).collect::<Vec<_>>()
    };
    connection
        .execute("DELETE FROM lectures WHERE id = ?1", [lecture_id])
        .map_err(|error| error.to_string())?;
    Ok((title, paths))
}

#[tauri::command]
pub fn delete_lecture(database: State<'_, Database>, lecture_id: i64) -> Result<String, String> {
    let (title, paths) = {
        let mut connection = lock_database(&database)?;
        delete_lecture_inner(&mut connection, lecture_id)?
    };
    let directories = paths
        .iter()
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect::<HashSet<_>>();
    for path in paths {
        let _ = fs::remove_file(path);
    }
    for directory in directories {
        let _ = fs::remove_dir(directory);
    }
    Ok(title)
}

#[tauri::command]
pub fn get_lecture(
    database: State<'_, Database>,
    lecture_id: i64,
) -> Result<LectureSnapshot, String> {
    let connection = lock_database(&database)?;
    let lecture = read_lecture_row(&connection, lecture_id)?;
    load_lecture_snapshot(&connection, lecture)
}

#[tauri::command]
pub fn add_lecture_bookmark(
    database: State<'_, Database>,
    lecture_id: i64,
    segment_id: Option<String>,
    timestamp_ms: i64,
    note: String,
) -> Result<LectureBookmark, String> {
    let connection = lock_database(&database)?;
    let created_at = chrono_timestamp_ms();
    connection
        .execute(
            "INSERT INTO lecture_bookmarks (lecture_id, segment_id, timestamp_ms, note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                lecture_id,
                segment_id,
                timestamp_ms,
                note.trim(),
                created_at
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(LectureBookmark {
        id: connection.last_insert_rowid(),
        lecture_id,
        segment_id,
        timestamp_ms,
        note: note.trim().to_string(),
        created_at,
    })
}

#[tauri::command]
pub fn list_lecture_bookmarks(
    database: State<'_, Database>,
    lecture_id: i64,
) -> Result<Vec<LectureBookmark>, String> {
    let connection = lock_database(&database)?;
    let mut statement = connection
        .prepare(
            "SELECT id, lecture_id, segment_id, timestamp_ms, note, created_at
             FROM lecture_bookmarks WHERE lecture_id = ?1 ORDER BY timestamp_ms, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([lecture_id], |row| {
            Ok(LectureBookmark {
                id: row.get(0)?,
                lecture_id: row.get(1)?,
                segment_id: row.get(2)?,
                timestamp_ms: row.get(3)?,
                note: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

#[tauri::command]
pub fn delete_lecture_bookmark(
    database: State<'_, Database>,
    bookmark_id: i64,
) -> Result<(), String> {
    let connection = lock_database(&database)?;
    connection
        .execute("DELETE FROM lecture_bookmarks WHERE id = ?1", [bookmark_id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn update_lecture_bookmark(
    database: State<'_, Database>,
    bookmark_id: i64,
    note: String,
) -> Result<LectureBookmark, String> {
    let connection = lock_database(&database)?;
    connection
        .execute(
            "UPDATE lecture_bookmarks SET note = ?1 WHERE id = ?2",
            params![note.trim(), bookmark_id],
        )
        .map_err(|error| error.to_string())?;
    connection
        .query_row(
            "SELECT id, lecture_id, segment_id, timestamp_ms, note, created_at
             FROM lecture_bookmarks WHERE id = ?1",
            [bookmark_id],
            |row| {
                Ok(LectureBookmark {
                    id: row.get(0)?,
                    lecture_id: row.get(1)?,
                    segment_id: row.get(2)?,
                    timestamp_ms: row.get(3)?,
                    note: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_recoverable_lecture(
    database: State<'_, Database>,
) -> Result<Option<LectureSnapshot>, String> {
    let connection = lock_database(&database)?;
    let lecture = connection
        .query_row(
            "SELECT id, course_id, title, started_at, elapsed_ms, status
             FROM lectures WHERE status != 'ended' ORDER BY started_at DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let Some(lecture) = lecture else {
        return Ok(None);
    };
    load_lecture_snapshot(&connection, lecture).map(Some)
}

fn chrono_timestamp_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_all_workspace_tables() {
        let database = Database::in_memory().expect("database");
        let connection = database.0.lock().expect("lock");
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN
                 ('courses', 'glossary_terms', 'lectures', 'transcript_segments', 'topic_summaries',
                  'lecture_summaries', 'lecture_bookmarks', 'course_documents', 'document_chunks',
                  'chat_threads', 'chat_messages')",
                [],
                |row| row.get(0),
            )
            .expect("table count");
        assert_eq!(count, 11);
    }

    #[test]
    fn initialization_removes_unreachable_legacy_rows() {
        let connection = Connection::open_in_memory().expect("database");
        initialize(&connection).expect("schema");
        connection
            .execute_batch("PRAGMA foreign_keys = OFF;")
            .expect("disable foreign keys");
        connection
            .execute(
                "INSERT INTO transcript_segments
                 (id, lecture_id, seq, start_ms, english, status)
                 VALUES ('orphan', 999, 0, 0, 'legacy test row', 'complete')",
                [],
            )
            .expect("legacy orphan");

        initialize(&connection).expect("repair");

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM transcript_segments", [], |row| {
                row.get(0)
            })
            .expect("segment count");
        assert_eq!(count, 0);
    }

    #[test]
    fn initialization_repairs_transient_segments_from_ended_lectures() {
        let connection = Connection::open_in_memory().expect("database");
        initialize(&connection).expect("schema");
        connection
            .execute(
                "INSERT INTO courses (name, created_at) VALUES ('EC1101E', 1)",
                [],
            )
            .expect("course");
        connection
            .execute(
                "INSERT INTO lectures (course_id, title, started_at, status)
                 VALUES (1, 'Lecture 01', 1, 'ended')",
                [],
            )
            .expect("lecture");
        connection
            .execute(
                "INSERT INTO transcript_segments
                 (id, lecture_id, seq, start_ms, english, status)
                 VALUES ('old-interim', 1, 0, 0, 'partial', 'interim'),
                        ('complete', 1, 1, 1000, 'finished', 'complete'),
                        ('old-translating', 1, 2, 2000, 'keep me', 'translating')",
                [],
            )
            .expect("segments");

        initialize(&connection).expect("repair");

        let remaining = connection
            .prepare(
                "SELECT id, status FROM transcript_segments
                 WHERE lecture_id = 1 ORDER BY seq",
            )
            .expect("statement")
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .expect("segments")
            .collect::<Result<Vec<_>, _>>()
            .expect("remaining segments");
        assert_eq!(
            remaining,
            vec![
                ("complete".to_string(), "complete".to_string()),
                ("old-translating".to_string(), "error".to_string())
            ]
        );
    }

    #[test]
    fn snapshot_upserts_transcript_text() {
        let database = Database::in_memory().expect("database");
        let mut connection = database.0.lock().expect("lock");
        connection
            .execute(
                "INSERT INTO courses (name, created_at) VALUES ('EC1101E', 1)",
                [],
            )
            .expect("course");
        connection
            .execute(
                "INSERT INTO lectures (course_id, title, started_at, status) VALUES (1, 'Lecture 01', 1, 'live')",
                [],
            )
            .expect("lecture");

        let make_snapshot = |chinese: &str| LectureSnapshotInput {
            lecture_id: 1,
            elapsed_ms: 2000,
            status: "live".to_string(),
            segments: vec![TranscriptSegmentInput {
                id: "segment-1".to_string(),
                start_ms: 500,
                english: "Demand rises.".to_string(),
                chinese: chinese.to_string(),
                state: "complete".to_string(),
            }],
            summaries: vec![],
        };

        save_lecture_snapshot_inner(&mut connection, make_snapshot("需求")).expect("first save");
        save_lecture_snapshot_inner(&mut connection, make_snapshot("需求上升。"))
            .expect("second save");
        let chinese: String = connection
            .query_row(
                "SELECT chinese FROM transcript_segments WHERE lecture_id = 1 AND id = 'segment-1'",
                [],
                |row| row.get(0),
            )
            .expect("translation");
        assert_eq!(chinese, "需求上升。");
    }

    #[test]
    fn snapshot_removes_superseded_interims_and_summaries() {
        let database = Database::in_memory().expect("database");
        let mut connection = database.0.lock().expect("lock");
        connection
            .execute(
                "INSERT INTO courses (name, created_at) VALUES ('CEG5104', 1)",
                [],
            )
            .expect("course");
        connection
            .execute(
                "INSERT INTO lectures (course_id, title, started_at, status)
                 VALUES (1, 'Lecture 01', 1, 'live')",
                [],
            )
            .expect("lecture");
        let summary = |id: &str| {
            serde_json::from_value::<TopicSummaryInput>(serde_json::json!({
                "id": id,
                "kind": "lecture",
                "startMs": 0,
                "title": id
            }))
            .expect("summary")
        };
        let snapshot = |interim_id: &str, summary_id: &str| LectureSnapshotInput {
            lecture_id: 1,
            elapsed_ms: 2_000,
            status: "live".to_string(),
            segments: vec![TranscriptSegmentInput {
                id: interim_id.to_string(),
                start_ms: 500,
                english: "partial".to_string(),
                chinese: String::new(),
                state: "interim".to_string(),
            }],
            summaries: vec![summary(summary_id)],
        };

        save_lecture_snapshot_inner(&mut connection, snapshot("old-item", "old-summary"))
            .expect("first snapshot");
        save_lecture_snapshot_inner(&mut connection, snapshot("active-item", "new-summary"))
            .expect("replacement snapshot");

        let stale_segments: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM transcript_segments WHERE id = 'old-item'",
                [],
                |row| row.get(0),
            )
            .expect("stale segment count");
        let stale_summaries: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM topic_summaries WHERE id = 'old-summary'",
                [],
                |row| row.get(0),
            )
            .expect("stale summary count");
        assert_eq!(stale_segments, 0);
        assert_eq!(stale_summaries, 0);
    }

    #[test]
    fn saves_translation_for_one_segment_without_overwriting_other_fields() {
        let database = Database::in_memory().expect("database");
        let mut connection = database.0.lock().expect("lock");
        connection
            .execute(
                "INSERT INTO courses (name, created_at) VALUES ('CS101', 1)",
                [],
            )
            .expect("course");
        connection
            .execute(
                "INSERT INTO lectures (course_id, title, started_at, status, elapsed_ms)
                 VALUES (1, 'Lecture 01', 1, 'ended', 4200),
                        (1, 'Lecture 02', 2, 'ended', 6000)",
                [],
            )
            .expect("lecture");
        connection
            .execute(
                "INSERT INTO transcript_segments
                 (id, lecture_id, seq, start_ms, end_ms, english, chinese, status)
                 VALUES ('first', 1, 3, 1200, 1800, 'Original one', '旧译文一', 'error'),
                        ('second', 1, 4, 2000, 2600, 'Original two', '旧译文二', 'complete'),
                        ('first', 2, 0, 100, 200, 'Other lecture', '另一堂课', 'complete')",
                [],
            )
            .expect("segments");
        connection
            .execute(
                "INSERT INTO topic_summaries (id, lecture_id, start_ms, title, content_json)
                 VALUES ('summary-1', 1, 1000, 'Original summary', '{}')",
                [],
            )
            .expect("summary");
        connection
            .execute(
                "INSERT INTO lecture_summaries (lecture_id, content_json, model, generated_at)
                 VALUES (1, '{\"overview\":\"original\"}', 'original-model', 3000)",
                [],
            )
            .expect("lecture summary");
        let before = serde_json::to_value(
            load_lecture_snapshot(
                &connection,
                read_lecture_row(&connection, 1).expect("lecture"),
            )
            .expect("snapshot"),
        )
        .expect("snapshot value");

        save_transcript_translation_inner(&mut connection, 1, "first", "新的译文一", "complete")
            .expect("translation");
        let after = serde_json::to_value(
            load_lecture_snapshot(
                &connection,
                read_lecture_row(&connection, 1).expect("lecture"),
            )
            .expect("snapshot"),
        )
        .expect("snapshot value");
        let mut expected = before;
        expected["segments"][0]["chinese"] = serde_json::json!("新的译文一");
        expected["segments"][0]["state"] = serde_json::json!("complete");
        assert_eq!(after, expected);
        let other_translation: String = connection
            .query_row(
                "SELECT chinese FROM transcript_segments WHERE lecture_id = 2 AND id = 'first'",
                [],
                |row| row.get(0),
            )
            .expect("other lecture translation");
        assert_eq!(other_translation, "另一堂课");
        let lecture_summary: (String, String, i64) = connection
            .query_row(
                "SELECT content_json, model, generated_at FROM lecture_summaries WHERE lecture_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("unchanged lecture summary");
        assert_eq!(
            lecture_summary,
            (
                "{\"overview\":\"original\"}".to_string(),
                "original-model".to_string(),
                3000
            )
        );

        let rows = connection
            .prepare(
                "SELECT id, seq, start_ms, end_ms, english, chinese, status
                 FROM transcript_segments WHERE lecture_id = 1 ORDER BY seq",
            )
            .expect("statement")
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .expect("rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        assert_eq!(
            rows,
            vec![
                (
                    "first".to_string(),
                    3,
                    1200,
                    Some(1800),
                    "Original one".to_string(),
                    "新的译文一".to_string(),
                    "complete".to_string()
                ),
                (
                    "second".to_string(),
                    4,
                    2000,
                    Some(2600),
                    "Original two".to_string(),
                    "旧译文二".to_string(),
                    "complete".to_string()
                )
            ]
        );
    }

    #[test]
    fn rejects_interim_missing_and_invalid_translation_updates() {
        let database = Database::in_memory().expect("database");
        let mut connection = database.0.lock().expect("lock");
        connection
            .execute(
                "INSERT INTO courses (name, created_at) VALUES ('CS101', 1)",
                [],
            )
            .expect("course");
        connection
            .execute(
                "INSERT INTO lectures (course_id, title, started_at, status)
                 VALUES (1, 'Lecture 01', 1, 'live')",
                [],
            )
            .expect("lecture");
        connection
            .execute(
                "INSERT INTO transcript_segments
                 (id, lecture_id, seq, start_ms, english, status)
                 VALUES ('interim', 1, 0, 0, 'Partial', 'interim'),
                        ('complete', 1, 1, 100, 'Done', 'complete')",
                [],
            )
            .expect("segments");

        assert_eq!(
            save_transcript_translation_inner(
                &mut connection,
                1,
                "interim",
                "不应保存",
                "complete"
            )
            .expect_err("interim rejected"),
            "不能保存尚未定稿语段的翻译"
        );
        assert_eq!(
            save_transcript_translation_inner(&mut connection, 1, "missing", "不存在", "complete")
                .expect_err("missing segment rejected"),
            "没有找到要更新翻译的语段"
        );
        assert_eq!(
            save_transcript_translation_inner(
                &mut connection,
                1,
                "complete",
                "译文",
                "translating"
            )
            .expect_err("invalid state rejected"),
            "翻译状态只能是 complete 或 error"
        );
        assert_eq!(
            save_transcript_translation_inner(&mut connection, 999, "complete", "译文", "complete")
                .expect_err("missing lecture rejected"),
            "没有找到要更新翻译的课堂记录"
        );
    }

    #[test]
    fn rejects_oversized_translation_and_empty_completed_translation() {
        let database = Database::in_memory().expect("database");
        let mut connection = database.0.lock().expect("lock");
        connection
            .execute(
                "INSERT INTO courses (name, created_at) VALUES ('CS101', 1)",
                [],
            )
            .expect("course");
        connection
            .execute(
                "INSERT INTO lectures (course_id, title, started_at, status)
                 VALUES (1, 'Lecture 01', 1, 'ended')",
                [],
            )
            .expect("lecture");
        connection
            .execute(
                "INSERT INTO transcript_segments
                 (id, lecture_id, seq, start_ms, english, status)
                 VALUES ('complete', 1, 0, 0, 'Done', 'complete')",
                [],
            )
            .expect("segment");

        let oversized = "x".repeat(32_001);
        assert_eq!(
            save_transcript_translation_inner(
                &mut connection,
                1,
                "complete",
                &oversized,
                "complete"
            )
            .expect_err("oversized translation rejected"),
            "译文不能超过 32000 个字符"
        );
        assert_eq!(
            save_transcript_translation_inner(&mut connection, 1, "complete", "  ", "complete")
                .expect_err("empty completed translation rejected"),
            "翻译完成时译文不能为空"
        );
        for invalid_id in [" ".to_string(), "s".repeat(201)] {
            assert_eq!(
                save_transcript_translation_inner(
                    &mut connection,
                    1,
                    &invalid_id,
                    "译文",
                    "complete"
                )
                .expect_err("invalid segment ID rejected"),
                "语段 ID 不能为空且不能超过 200 个字符"
            );
        }
        let max_id = "s".repeat(200);
        connection
            .execute(
                "UPDATE transcript_segments SET id = ?1 WHERE lecture_id = 1 AND id = 'complete'",
                [&max_id],
            )
            .expect("boundary segment ID");
        let max_translation = "译".repeat(32_000);
        save_transcript_translation_inner(
            &mut connection,
            1,
            &max_id,
            &max_translation,
            "complete",
        )
        .expect("character length boundaries allowed");
        save_transcript_translation_inner(&mut connection, 1, &max_id, "", "error")
            .expect("empty failed translation allowed");
        let result: (String, String) = connection
            .query_row(
                "SELECT chinese, status FROM transcript_segments WHERE lecture_id = 1 AND id = ?1",
                [&max_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("failed result");
        assert_eq!(result, (String::new(), "error".to_string()));
    }

    #[test]
    fn deleting_an_ended_lecture_cascades_owned_content() {
        let database = Database::in_memory().expect("database");
        let mut connection = database.0.lock().expect("lock");
        connection
            .execute(
                "INSERT INTO courses (name, created_at) VALUES ('CEG5104', 1)",
                [],
            )
            .expect("course");
        connection
            .execute(
                "INSERT INTO lectures (course_id, title, started_at, ended_at, status)
                 VALUES (1, 'Empty test', 1, 2, 'ended')",
                [],
            )
            .expect("lecture");
        connection
            .execute(
                "INSERT INTO transcript_segments
                 (id, lecture_id, seq, start_ms, english, status)
                 VALUES ('segment-1', 1, 0, 0, 'Test', 'complete')",
                [],
            )
            .expect("segment");
        connection
            .execute(
                "INSERT INTO lecture_bookmarks (lecture_id, timestamp_ms, created_at)
                 VALUES (1, 0, 1)",
                [],
            )
            .expect("bookmark");

        let (title, _) = delete_lecture_inner(&mut connection, 1).expect("delete lecture");
        assert_eq!(title, "Empty test");
        let lecture_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM lectures", [], |row| row.get(0))
            .expect("lecture count");
        let segment_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM transcript_segments", [], |row| {
                row.get(0)
            })
            .expect("segment count");
        let bookmark_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM lecture_bookmarks", [], |row| {
                row.get(0)
            })
            .expect("bookmark count");
        assert_eq!((lecture_count, segment_count, bookmark_count), (0, 0, 0));
    }

    #[test]
    fn refuses_to_delete_an_active_lecture() {
        let database = Database::in_memory().expect("database");
        let mut connection = database.0.lock().expect("lock");
        connection
            .execute(
                "INSERT INTO courses (name, created_at) VALUES ('CEG5104', 1)",
                [],
            )
            .expect("course");
        connection
            .execute(
                "INSERT INTO lectures (course_id, title, started_at, status)
                 VALUES (1, 'Current lecture', 1, 'live')",
                [],
            )
            .expect("lecture");
        assert_eq!(
            delete_lecture_inner(&mut connection, 1).expect_err("active lecture rejected"),
            "只能删除已经结束的课堂记录"
        );
    }
}
