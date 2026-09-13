use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;
use tauri::State;

use crate::storage::Database;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub course_id: i64,
    pub lecture_id: Option<i64>,
    pub scope: String,
    pub question: String,
    pub provider: String,
    pub workspace_id: String,
    pub model: String,
    pub web_search: bool,
    #[serde(default)]
    pub openai_base_url: Option<String>,
    #[serde(default)]
    pub key_slot: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCitation {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub excerpt: String,
    pub lecture_id: Option<i64>,
    pub segment_id: Option<String>,
    pub timestamp_ms: Option<i64>,
    pub document_id: Option<i64>,
    pub page_number: Option<i64>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatThread {
    pub id: i64,
    pub course_id: i64,
    pub lecture_id: Option<i64>,
    pub scope: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: i64,
    pub thread_id: i64,
    pub role: String,
    pub content: String,
    pub citations: Vec<ChatCitation>,
    pub model: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAnswer {
    pub thread: ChatThread,
    pub message: ChatMessage,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistory {
    pub thread: Option<ChatThread>,
    pub messages: Vec<ChatMessage>,
}

fn timestamp_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn validate_scope(scope: &str, lecture_id: Option<i64>) -> Result<(), String> {
    if !matches!(scope, "lecture" | "course") {
        return Err("不支持的问答范围".to_string());
    }
    if scope == "lecture" && lecture_id.is_none() {
        return Err("本节课问答需要先选择一节课堂".to_string());
    }
    if scope == "course" && lecture_id.is_some() {
        return Err("本课程问答不能绑定单节课堂".to_string());
    }
    Ok(())
}

fn validate_context_ids(
    connection: &Connection,
    course_id: i64,
    lecture_id: Option<i64>,
) -> Result<(), String> {
    let course_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM courses WHERE id = ?1)",
            [course_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| error.to_string())?;
    if !course_exists {
        return Err("没有找到这门课程".to_string());
    }
    if let Some(lecture_id) = lecture_id {
        let lecture_course_id = connection
            .query_row(
                "SELECT course_id FROM lectures WHERE id = ?1",
                [lecture_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "没有找到这节课堂".to_string())?;
        if lecture_course_id != course_id {
            return Err("所选课堂不属于当前课程".to_string());
        }
    }
    Ok(())
}

fn clip_text(value: &str, max_characters: usize) -> String {
    let mut characters = value.chars();
    let clipped = characters.by_ref().take(max_characters).collect::<String>();
    if characters.next().is_some() {
        format!("{clipped}\n…")
    } else {
        clipped
    }
}

fn thread_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatThread> {
    Ok(ChatThread {
        id: row.get(0)?,
        course_id: row.get(1)?,
        lecture_id: row.get(2)?,
        scope: row.get(3)?,
        title: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn get_thread(
    connection: &Connection,
    course_id: i64,
    lecture_id: Option<i64>,
    scope: &str,
) -> Result<Option<ChatThread>, String> {
    connection
        .query_row(
            "SELECT id, course_id, lecture_id, scope, title, created_at, updated_at
             FROM chat_threads
             WHERE course_id = ?1 AND COALESCE(lecture_id, -1) = COALESCE(?2, -1) AND scope = ?3
             ORDER BY updated_at DESC, id DESC LIMIT 1",
            params![course_id, lecture_id, scope],
            thread_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn get_or_create_thread(
    connection: &Connection,
    request: &ChatRequest,
) -> Result<ChatThread, String> {
    if let Some(thread) = get_thread(
        connection,
        request.course_id,
        request.lecture_id,
        &request.scope,
    )? {
        return Ok(thread);
    }
    let now = timestamp_ms();
    let title = request.question.chars().take(48).collect::<String>();
    connection
        .execute(
            "INSERT INTO chat_threads (course_id, lecture_id, scope, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![
                request.course_id,
                request.lecture_id,
                request.scope,
                title,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
    let id = connection.last_insert_rowid();
    Ok(ChatThread {
        id,
        course_id: request.course_id,
        lecture_id: request.lecture_id,
        scope: request.scope.clone(),
        title,
        created_at: now,
        updated_at: now,
    })
}

fn insert_message(
    connection: &Connection,
    thread_id: i64,
    role: &str,
    content: &str,
    citations: &[ChatCitation],
    model: Option<&str>,
) -> Result<ChatMessage, String> {
    let created_at = timestamp_ms();
    let citations_json = serde_json::to_string(citations).map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO chat_messages (thread_id, role, content, citations_json, model, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![thread_id, role, content, citations_json, model, created_at],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE chat_threads SET updated_at = ?1 WHERE id = ?2",
            params![created_at, thread_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(ChatMessage {
        id: connection.last_insert_rowid(),
        thread_id,
        role: role.to_string(),
        content: content.to_string(),
        citations: citations.to_vec(),
        model: model.map(str::to_string),
        created_at,
    })
}

fn messages_for_thread(
    connection: &Connection,
    thread_id: i64,
) -> Result<Vec<ChatMessage>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, thread_id, role, content, citations_json, model, created_at
             FROM chat_messages WHERE thread_id = ?1 ORDER BY created_at, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([thread_id], |row| {
            let citations_json: String = row.get(4)?;
            let citations =
                serde_json::from_str::<Vec<ChatCitation>>(&citations_json).unwrap_or_default();
            let role: String = row.get(2)?;
            let content: String = row.get(3)?;
            Ok(ChatMessage {
                id: row.get(0)?,
                thread_id: row.get(1)?,
                content: if role == "assistant" {
                    sanitize_local_citation_markers(&content, &citations)
                } else {
                    content
                },
                role,
                citations,
                model: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn query_terms(question: &str) -> Vec<String> {
    let mut terms = question
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() >= 2)
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    let cjk = question
        .chars()
        .filter(|character| !character.is_ascii() && !character.is_whitespace())
        .collect::<Vec<_>>();
    for window in cjk.windows(2) {
        terms.push(window.iter().collect());
    }
    terms.sort();
    terms.dedup();
    terms
}

fn relevance(content: &str, terms: &[String]) -> i64 {
    let lower = content.to_ascii_lowercase();
    terms
        .iter()
        .map(|term| {
            if lower.contains(term) {
                1 + term.chars().count() as i64 / 6
            } else {
                0
            }
        })
        .sum()
}

fn transcript_sources(
    connection: &Connection,
    request: &ChatRequest,
    terms: &[String],
) -> Result<Vec<(i64, ChatCitation, String)>, String> {
    let query = if request.scope == "lecture" {
        "SELECT t.id, t.lecture_id, t.start_ms, t.english, t.chinese, l.title
         FROM transcript_segments t JOIN lectures l ON l.id = t.lecture_id
         WHERE t.lecture_id = ?1 AND t.status != 'interim' ORDER BY t.seq"
    } else {
        "SELECT t.id, t.lecture_id, t.start_ms, t.english, t.chinese, l.title
         FROM transcript_segments t JOIN lectures l ON l.id = t.lecture_id
         WHERE l.course_id = ?1 AND t.status != 'interim' ORDER BY l.started_at DESC, t.seq"
    };
    let id = if request.scope == "lecture" {
        request.lecture_id.unwrap_or_default()
    } else {
        request.course_id
    };
    let mut statement = connection
        .prepare(query)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut ranked = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(
            |(segment_id, lecture_id, start_ms, english, chinese, lecture_title)| {
                let content = clip_text(&format!("{english}\n{chinese}"), 1_600);
                let score = relevance(&content, terms);
                (
                    score,
                    ChatCitation {
                        kind: "transcript".to_string(),
                        title: format!(
                            "{} · {:02}:{:02}",
                            lecture_title,
                            start_ms / 60_000,
                            start_ms / 1_000 % 60
                        ),
                        excerpt: content.chars().take(300).collect(),
                        lecture_id: Some(lecture_id),
                        segment_id: Some(segment_id),
                        timestamp_ms: Some(start_ms),
                        ..Default::default()
                    },
                    content,
                )
            },
        )
        .filter(|item| item.0 > 0)
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.0.cmp(&left.0));
    ranked.truncate(8);
    Ok(ranked)
}

fn document_sources(
    connection: &Connection,
    request: &ChatRequest,
    terms: &[String],
) -> Result<Vec<(i64, ChatCitation, String)>, String> {
    let query = if request.scope == "lecture" {
        "SELECT c.document_id, d.lecture_id, d.name, c.page_number, c.heading, c.content
         FROM document_chunks c JOIN course_documents d ON d.id = c.document_id
         WHERE d.lecture_id = ?1"
    } else {
        "SELECT c.document_id, d.lecture_id, d.name, c.page_number, c.heading, c.content
         FROM document_chunks c JOIN course_documents d ON d.id = c.document_id
         WHERE d.course_id = ?1"
    };
    let id = if request.scope == "lecture" {
        request.lecture_id.unwrap_or_default()
    } else {
        request.course_id
    };
    let mut statement = connection
        .prepare(query)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut ranked = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(
            |(document_id, lecture_id, name, page_number, heading, content)| {
                let source_text = clip_text(&format!("{heading}\n{content}"), 5_000);
                let score = relevance(&source_text, terms);
                (
                    score,
                    ChatCitation {
                        kind: "slide".to_string(),
                        title: format!("{name} · P.{page_number}"),
                        excerpt: source_text.chars().take(320).collect(),
                        lecture_id,
                        document_id: Some(document_id),
                        page_number: Some(page_number),
                        ..Default::default()
                    },
                    source_text,
                )
            },
        )
        .filter(|item| item.0 > 0)
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.0.cmp(&left.0));
    ranked.truncate(6);
    Ok(ranked)
}

fn source_context(
    connection: &Connection,
    request: &ChatRequest,
) -> Result<(String, Vec<ChatCitation>), String> {
    let terms = query_terms(&request.question);
    let mut ranked = transcript_sources(connection, request, &terms)?;
    ranked.extend(document_sources(connection, request, &terms)?);
    ranked.sort_by(|left, right| right.0.cmp(&left.0));
    ranked.truncate(12);
    let mut citations = Vec::new();
    let mut blocks = Vec::new();
    let mut context_characters = 0usize;
    for (index, (_, mut citation, content)) in ranked.into_iter().enumerate() {
        let prefix = if citation.kind == "transcript" {
            "T"
        } else {
            "S"
        };
        citation.id = format!("{prefix}{}", index + 1);
        let block = format!("[{}] {}\n{}", citation.id, citation.title, content);
        let block_characters = block.chars().count();
        if context_characters + block_characters > 36_000 {
            break;
        }
        context_characters += block_characters;
        blocks.push(block);
        citations.push(citation);
    }
    let syllabus: Option<(String, String)> = connection
        .query_row(
            "SELECT name, description FROM courses WHERE id = ?1",
            [request.course_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some((course_name, syllabus)) = syllabus.filter(|(_, text)| !text.trim().is_empty()) {
        let syllabus = clip_text(&syllabus, 8_000);
        let citation = ChatCitation {
            id: "Y1".to_string(),
            kind: "syllabus".to_string(),
            title: format!("{course_name} · Syllabus"),
            excerpt: syllabus.chars().take(360).collect(),
            ..Default::default()
        };
        blocks.push(format!("[Y1] {}\n{}", citation.title, syllabus));
        citations.push(citation);
    }
    Ok((blocks.join("\n\n"), citations))
}

fn output_text(response: &Value) -> Option<String> {
    if let Some(text) = response.get("output_text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    response
        .get("output")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .find_map(|content| {
            (content.get("type").and_then(Value::as_str) == Some("output_text"))
                .then(|| {
                    content
                        .get("text")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .flatten()
        })
}

fn web_sources(response: &Value) -> Vec<ChatCitation> {
    let mut urls = Vec::<(String, String)>::new();
    let Some(output) = response.get("output").and_then(Value::as_array) else {
        return vec![];
    };
    for item in output {
        if let Some(sources) = item
            .get("action")
            .and_then(|action| action.get("sources"))
            .and_then(Value::as_array)
        {
            for source in sources {
                if let Some(url) = source.get("url").and_then(Value::as_str) {
                    let title = source.get("title").and_then(Value::as_str).unwrap_or(url);
                    urls.push((title.to_string(), url.to_string()));
                }
            }
        }
        if let Some(content) = item.get("content").and_then(Value::as_array) {
            for part in content {
                if let Some(annotations) = part.get("annotations").and_then(Value::as_array) {
                    for annotation in annotations {
                        let citation = annotation.get("url_citation").unwrap_or(annotation);
                        if let Some(url) = citation.get("url").and_then(Value::as_str) {
                            let title =
                                citation.get("title").and_then(Value::as_str).unwrap_or(url);
                            urls.push((title.to_string(), url.to_string()));
                        }
                    }
                }
            }
        }
    }
    let mut seen = HashSet::new();
    urls.into_iter()
        .filter(|(_, url)| url.starts_with("https://") && seen.insert(url.clone()))
        .take(8)
        .enumerate()
        .map(|(index, (title, url))| ChatCitation {
            id: format!("W{}", index + 1),
            kind: "web".to_string(),
            title,
            excerpt: url.clone(),
            url: Some(url),
            ..Default::default()
        })
        .collect()
}

fn sanitize_local_citation_markers(answer: &str, allowed: &[ChatCitation]) -> String {
    let allowed_ids = allowed
        .iter()
        .map(|citation| citation.id.as_str())
        .collect::<HashSet<_>>();
    let mut sanitized = String::with_capacity(answer.len());
    let mut cursor = 0usize;
    while let Some(relative_start) = answer[cursor..].find('[') {
        let start = cursor + relative_start;
        sanitized.push_str(&answer[cursor..start]);
        let Some(relative_end) = answer[start + 1..].find(']') else {
            cursor = start;
            break;
        };
        let end = start + 1 + relative_end;
        let marker = &answer[start + 1..end];
        let is_local_marker = marker
            .chars()
            .next()
            .is_some_and(|prefix| matches!(prefix, 'T' | 'S' | 'Y'))
            && marker
                .chars()
                .skip(1)
                .all(|character| character.is_ascii_digit())
            && marker.len() > 1;
        if !is_local_marker || allowed_ids.contains(marker) {
            sanitized.push_str(&answer[start..=end]);
        }
        cursor = end + 1;
    }
    sanitized.push_str(&answer[cursor..]);
    sanitized
}

fn verified_answer_citations(
    answer: String,
    allowed: Vec<ChatCitation>,
    web: Vec<ChatCitation>,
) -> (String, Vec<ChatCitation>) {
    let mut answer = sanitize_local_citation_markers(&answer, &allowed);
    let mut citations = allowed
        .iter()
        .filter(|citation| answer.contains(&format!("[{}]", citation.id)))
        .cloned()
        .collect::<Vec<_>>();
    if citations.is_empty() && !allowed.is_empty() {
        citations = allowed.into_iter().take(2).collect();
        let markers = citations
            .iter()
            .map(|citation| format!("[{}]", citation.id))
            .collect::<Vec<_>>()
            .join(" ");
        answer.push_str(&format!("\n\n参考课堂资料：{markers}"));
    }
    citations.extend(web);
    (answer, citations)
}

#[tauri::command]
pub fn get_chat_history(
    database: State<'_, Database>,
    course_id: i64,
    lecture_id: Option<i64>,
    scope: String,
) -> Result<ChatHistory, String> {
    validate_scope(&scope, lecture_id)?;
    let connection = database
        .0
        .lock()
        .map_err(|_| "课堂数据库锁已损坏".to_string())?;
    validate_context_ids(&connection, course_id, lecture_id)?;
    let thread = get_thread(&connection, course_id, lecture_id, &scope)?;
    let messages = thread
        .as_ref()
        .map(|thread| messages_for_thread(&connection, thread.id))
        .transpose()?
        .unwrap_or_default();
    Ok(ChatHistory { thread, messages })
}

#[tauri::command]
pub async fn ask_lecture_chat(
    database: State<'_, Database>,
    request: ChatRequest,
) -> Result<ChatAnswer, String> {
    validate_scope(&request.scope, request.lecture_id)?;
    if request.question.trim().is_empty() {
        return Err("请输入问题".to_string());
    }
    if request.question.chars().count() > 8_000 {
        return Err("问题内容过长，请缩短到 8000 字以内".to_string());
    }
    let (context, allowed_citations, recent_messages) = {
        let connection = database
            .0
            .lock()
            .map_err(|_| "课堂数据库锁已损坏".to_string())?;
        validate_context_ids(&connection, request.course_id, request.lecture_id)?;
        let (context, citations) = source_context(&connection, &request)?;
        let history = match get_thread(
            &connection,
            request.course_id,
            request.lecture_id,
            &request.scope,
        )? {
            Some(thread) => messages_for_thread(&connection, thread.id)?,
            None => Vec::new(),
        };
        let recent = history
            .iter()
            .rev()
            .take(8)
            .rev()
            .map(|message| format!("{}: {}", message.role, clip_text(&message.content, 3_000)))
            .collect::<Vec<_>>()
            .join("\n");
        (context, citations, recent)
    };

    let instructions = "你是 NUS 课堂学习助理。优先依据提供的课堂转写、Slides 和 Syllabus 回答，必要时使用联网结果或模型通用知识。引用本地资料时必须在对应句子后使用给定编号，例如 [T1]、[S2]、[Y1]，不得编造编号。使用模型通用知识且没有资料支持时明确标注‘模型通用知识，未由课堂资料核验’。回答使用简体中文，先直接回答，再解释；不得声称教师讲过输入中不存在的内容。使用规范 Markdown 组织标题、加粗、列表和表格，不要用反斜杠转义 Markdown 标记，不要输出 HTML。行内公式必须写在 $...$ 中，独立公式必须写在 $$...$$ 中，并使用标准 LaTeX 表达式。";
    let input = format!(
        "资料范围：{}\n\n可引用资料：\n{}\n\n最近对话：\n{}\n\n用户问题：{}",
        request.scope,
        if context.is_empty() {
            "（未检索到相关本地资料）"
        } else {
            &context
        },
        if recent_messages.is_empty() {
            "（无）"
        } else {
            &recent_messages
        },
        request.question.trim()
    );
    let base_url = if request.provider == "openai" { crate::custom_openai_base_url(request.openai_base_url.as_deref())? } else { crate::provider_base_url(&request.provider, &request.workspace_id)? };
    let endpoint = format!("{base_url}/responses");
    let mut payload = json!({
        "model": request.model,
        "instructions": instructions,
        "input": input,
        "max_output_tokens": 2200,
        "store": false,
        "reasoning": {"effort": "none"}
    });
    if request.web_search {
        payload["tools"] = json!([{"type": "web_search"}]);
        payload["tool_choice"] = json!("auto");
        if request.provider == "openai" {
            payload["include"] = json!(["web_search_call.action.sources"]);
        }
    }
    let api_key = crate::read_provider_api_key_slot(&request.provider, request.key_slot.as_deref())?;
    let retry_delays = [250, 750, 1_500];
    let mut retry = 0usize;
    let response = loop {
        let retry_client = (retry > 0).then(crate::build_http_client);
        let client = retry_client
            .as_ref()
            .unwrap_or_else(|| crate::http_client());
        let mut request_builder = client
            .post(&endpoint)
            .bearer_auth(&api_key)
            .header("Content-Type", "application/json")
            .json(&payload);
        if request.provider == "openai" {
            request_builder = request_builder.header(
                "OpenAI-Safety-Identifier",
                "nus-lecture-assistant-local-user",
            );
        }
        match request_builder.send().await {
            Ok(response) => break response,
            Err(error)
                if retry < retry_delays.len()
                    && (error.is_connect() || error.is_timeout() || error.is_request()) =>
            {
                tokio::time::sleep(Duration::from_millis(retry_delays[retry])).await;
                retry += 1;
            }
            Err(error) => {
                return Err(crate::provider_connection_error(
                    if request.provider == "alibaba" {
                        "阿里云百炼"
                    } else {
                        "OpenAI"
                    },
                    "AI 问答",
                    &error,
                    retry,
                    request.provider == "alibaba" && request.workspace_id.trim().is_empty(),
                ))
            }
        }
    };
    let status = response.status();
    let body = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(crate::compact_error(&request.provider, &body, status));
    }
    let response: Value =
        serde_json::from_str(&body).map_err(|error| format!("问答响应格式无效：{error}"))?;
    let answer = output_text(&response).ok_or_else(|| "模型没有返回回答".to_string())?;
    let (answer, citations) =
        verified_answer_citations(answer, allowed_citations, web_sources(&response));
    let (thread, message) = {
        let mut connection = database
            .0
            .lock()
            .map_err(|_| "课堂数据库锁已损坏".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let mut thread = get_or_create_thread(&transaction, &request)?;
        insert_message(
            &transaction,
            thread.id,
            "user",
            request.question.trim(),
            &[],
            None,
        )?;
        let message = insert_message(
            &transaction,
            thread.id,
            "assistant",
            &answer,
            &citations,
            Some(&request.model),
        )?;
        transaction.commit().map_err(|error| error.to_string())?;
        thread.updated_at = message.created_at;
        (thread, message)
    };
    Ok(ChatAnswer { thread, message })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_terms_for_english_and_chinese_queries() {
        let terms = query_terms("Explain swarm intelligence 群体智能");
        assert!(terms.contains(&"swarm".to_string()));
        assert!(terms.contains(&"群体".to_string()));
    }

    #[test]
    fn only_accepts_real_citation_markers() {
        let (answer, citations) = verified_answer_citations(
            "教师在这里定义了算法。[T1]".to_string(),
            vec![
                ChatCitation {
                    id: "T1".to_string(),
                    ..Default::default()
                },
                ChatCitation {
                    id: "T2".to_string(),
                    ..Default::default()
                },
            ],
            vec![],
        );
        assert!(answer.contains("[T1]"));
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].id, "T1");
    }

    #[test]
    fn adds_real_sources_when_the_model_omits_markers() {
        let (answer, citations) = verified_answer_citations(
            "这是基于课堂资料的回答。".to_string(),
            vec![ChatCitation {
                id: "S1".to_string(),
                ..Default::default()
            }],
            vec![],
        );
        assert!(answer.ends_with("参考课堂资料：[S1]"));
        assert_eq!(citations[0].id, "S1");
    }

    #[test]
    fn removes_local_citation_markers_the_model_invented() {
        let (answer, citations) = verified_answer_citations(
            "课堂内容支持这一点。[T1] 但不是 [T99]。".to_string(),
            vec![ChatCitation {
                id: "T1".to_string(),
                ..Default::default()
            }],
            vec![],
        );

        assert!(answer.contains("[T1]"));
        assert!(!answer.contains("[T99]"));
        assert_eq!(citations.len(), 1);
    }
}
