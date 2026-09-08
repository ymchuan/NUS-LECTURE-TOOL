use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use quick_xml::{events::Event, Reader};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    fs,
    hash::{Hash, Hasher},
    io::{Cursor, Read},
    path::PathBuf,
};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;
use zip::ZipArchive;

use crate::storage::Database;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSlidesRequest {
    pub course_id: i64,
    pub lecture_id: i64,
    pub name: String,
    pub mime_type: String,
    pub data_base64: String,
    pub parsed_pages: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LectureDocument {
    pub id: i64,
    pub course_id: i64,
    pub lecture_id: Option<i64>,
    pub kind: String,
    pub name: String,
    pub mime_type: String,
    pub page_count: i64,
    pub parse_status: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentChunk {
    pub id: i64,
    pub document_id: i64,
    pub page_number: i64,
    pub heading: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSource {
    pub document_id: i64,
    pub name: String,
    pub mime_type: String,
    pub page_count: i64,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideMatch {
    pub document_id: i64,
    pub document_name: String,
    pub page_number: i64,
    pub heading: String,
    pub excerpt: String,
    pub score: i64,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
struct SlideCandidate {
    document_id: i64,
    document_name: String,
    page_number: i64,
    heading: String,
    content: String,
}

#[derive(Debug)]
struct RankedSlide<'a> {
    candidate: &'a SlideCandidate,
    total: f64,
    evidence: f64,
    phrase_score: f64,
    coverage: f64,
    matched_terms: usize,
}

impl RankedSlide<'_> {
    fn confidence(&self, competing_score: f64) -> f64 {
        let margin = ((self.total - competing_score) / self.total.max(1.0)).clamp(0.0, 1.0);
        (0.48 * (1.0 - (-self.evidence / 7.0).exp())
            + 0.28 * self.coverage.clamp(0.0, 1.0)
            + 0.18 * margin
            + if self.phrase_score > 0.0 { 0.06 } else { 0.0 }
            + if self.matched_terms >= 4 { 0.05 } else { 0.0 })
        .clamp(0.0, 1.0)
    }
}

fn timestamp_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn clean_text(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_pdf(bytes: &[u8]) -> Result<Vec<String>, String> {
    pdf_extract::extract_text_from_mem_by_pages(bytes)
        .map(|pages| pages.into_iter().map(|page| clean_text(&page)).collect())
        .map_err(|error| format!("无法解析 PDF：{error}"))
}

fn slide_number(name: &str) -> usize {
    name.strip_prefix("ppt/slides/slide")
        .and_then(|value| value.strip_suffix(".xml"))
        .and_then(|value| value.parse().ok())
        .unwrap_or(usize::MAX)
}

fn parse_slide_xml(xml: &[u8]) -> String {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut text = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(value)) => {
                if let Ok(decoded) = value.decode() {
                    let decoded = decoded.trim();
                    if !decoded.is_empty() {
                        text.push(decoded.to_string());
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    text.join("\n")
}

fn parse_pptx(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| format!("无法打开 PPTX：{error}"))?;
    if archive.len() > 10_000 {
        return Err("PPTX 内部文件数量异常，已停止解析".to_string());
    }
    let mut names = (0..archive.len())
        .filter_map(|index| {
            archive
                .by_index(index)
                .ok()
                .map(|file| file.name().to_string())
        })
        .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    names.sort_by_key(|name| slide_number(name));
    let mut slides = Vec::with_capacity(names.len());
    let mut total_xml_size = 0u64;
    for name in names {
        let mut file = archive.by_name(&name).map_err(|error| error.to_string())?;
        if file.size() > 10 * 1024 * 1024 {
            return Err(format!(
                "PPTX 第 {} 页内容异常大，已停止解析",
                slide_number(&name)
            ));
        }
        total_xml_size = total_xml_size.saturating_add(file.size());
        if total_xml_size > 100 * 1024 * 1024 {
            return Err("PPTX 解压后的页面内容超过 100MB，已停止解析".to_string());
        }
        let mut xml = Vec::new();
        file.read_to_end(&mut xml)
            .map_err(|error| error.to_string())?;
        slides.push(parse_slide_xml(&xml));
    }
    if slides.is_empty() {
        return Err("PPTX 中没有找到可读取的幻灯片".to_string());
    }
    Ok(slides)
}

fn parse_legacy_ppt(bytes: &[u8], parsed_pages: Option<&[String]>) -> Result<Vec<String>, String> {
    const CFB_SIGNATURE: &[u8; 8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";
    if bytes.get(..CFB_SIGNATURE.len()) != Some(CFB_SIGNATURE) {
        return Err("PPT 文件不是有效的 PowerPoint 97-2003 格式".to_string());
    }
    let pages = parsed_pages.ok_or_else(|| "PPT 缺少本地解析结果，请重新选择文件".to_string())?;
    if pages.is_empty() || pages.len() > 2_000 {
        return Err("PPT 幻灯片数量无效或超过 2000 页".to_string());
    }
    let mut total_characters = 0usize;
    let mut cleaned = Vec::with_capacity(pages.len());
    for (index, page) in pages.iter().enumerate() {
        let characters = page.chars().count();
        if characters > 1_000_000 {
            return Err(format!("PPT 第 {} 页文字内容异常大", index + 1));
        }
        total_characters = total_characters.saturating_add(characters);
        if total_characters > 10_000_000 {
            return Err("PPT 提取文字总量超过安全上限".to_string());
        }
        cleaned.push(clean_text(page));
    }
    Ok(cleaned)
}

fn heading_for(content: &str, page_number: usize) -> String {
    content
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().chars().take(90).collect())
        .unwrap_or_else(|| format!("第 {page_number} 页"))
}

fn document_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LectureDocument> {
    Ok(LectureDocument {
        id: row.get(0)?,
        course_id: row.get(1)?,
        lecture_id: row.get(2)?,
        kind: row.get(3)?,
        name: row.get(4)?,
        mime_type: row.get(5)?,
        page_count: row.get(6)?,
        parse_status: row.get(7)?,
        created_at: row.get(8)?,
    })
}

#[tauri::command]
pub fn import_lecture_slides(
    app: AppHandle,
    database: State<'_, Database>,
    request: ImportSlidesRequest,
) -> Result<LectureDocument, String> {
    let extension = request
        .name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .ok_or_else(|| "Slides 文件缺少扩展名".to_string())?;
    if !matches!(extension.as_str(), "pdf" | "pptx" | "ppt") {
        return Err("目前仅支持 PDF、PPTX 和 PPT Slides".to_string());
    }
    {
        let connection = database
            .0
            .lock()
            .map_err(|_| "课堂数据库锁已损坏".to_string())?;
        let valid_lecture = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM lectures WHERE id = ?1 AND course_id = ?2
                )",
                params![request.lecture_id, request.course_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| error.to_string())?;
        if !valid_lecture {
            return Err("所选课堂不属于当前课程".to_string());
        }
    }
    let bytes = BASE64
        .decode(request.data_base64.as_bytes())
        .map_err(|_| "Slides 文件内容无效".to_string())?;
    if bytes.is_empty() || bytes.len() > 50 * 1024 * 1024 {
        return Err("Slides 文件必须小于 50MB".to_string());
    }
    let pages = match extension.as_str() {
        "pdf" => parse_pdf(&bytes)?,
        "pptx" => parse_pptx(&bytes)?,
        "ppt" => parse_legacy_ppt(&bytes, request.parsed_pages.as_deref())?,
        _ => unreachable!(),
    };

    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let content_hash = format!("{:016x}", hasher.finish());
    let created_at = timestamp_ms();
    let relative = PathBuf::from("documents")
        .join(request.course_id.to_string())
        .join(request.lecture_id.to_string());
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join(&relative);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let stored_path = directory.join(format!("{}.{}", Uuid::new_v4(), extension));
    fs::write(&stored_path, &bytes).map_err(|error| format!("无法保存 Slides：{error}"))?;

    let database_result = (|| -> Result<i64, String> {
        let mut connection = database
            .0
            .lock()
            .map_err(|_| "课堂数据库锁已损坏".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO course_documents
                 (course_id, lecture_id, kind, name, mime_type, stored_path, content_hash, page_count, parse_status, created_at)
                 VALUES (?1, ?2, 'slides', ?3, ?4, ?5, ?6, ?7, 'ready', ?8)",
                params![
                    request.course_id,
                    request.lecture_id,
                    request.name,
                    request.mime_type,
                    stored_path.to_string_lossy(),
                    content_hash,
                    pages.len() as i64,
                    created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        let document_id = transaction.last_insert_rowid();
        for (index, content) in pages.iter().enumerate() {
            let page_number = index + 1;
            transaction
                .execute(
                    "INSERT INTO document_chunks (document_id, page_number, heading, content)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        document_id,
                        page_number as i64,
                        heading_for(content, page_number),
                        content
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(document_id)
    })();
    let document_id = match database_result {
        Ok(document_id) => document_id,
        Err(error) => {
            let _ = fs::remove_file(&stored_path);
            return Err(error);
        }
    };
    Ok(LectureDocument {
        id: document_id,
        course_id: request.course_id,
        lecture_id: Some(request.lecture_id),
        kind: "slides".to_string(),
        name: request.name,
        mime_type: request.mime_type,
        page_count: pages.len() as i64,
        parse_status: "ready".to_string(),
        created_at,
    })
}

#[tauri::command]
pub fn list_lecture_documents(
    database: State<'_, Database>,
    lecture_id: i64,
) -> Result<Vec<LectureDocument>, String> {
    let connection = database
        .0
        .lock()
        .map_err(|_| "课堂数据库锁已损坏".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT id, course_id, lecture_id, kind, name, mime_type, page_count, parse_status, created_at
             FROM course_documents WHERE lecture_id = ?1 ORDER BY created_at, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([lecture_id], document_from_row)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

#[tauri::command]
pub fn list_document_chunks(
    database: State<'_, Database>,
    document_id: i64,
) -> Result<Vec<DocumentChunk>, String> {
    let connection = database
        .0
        .lock()
        .map_err(|_| "课堂数据库锁已损坏".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT id, document_id, page_number, heading, content
             FROM document_chunks WHERE document_id = ?1 ORDER BY page_number",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([document_id], |row| {
            Ok(DocumentChunk {
                id: row.get(0)?,
                document_id: row.get(1)?,
                page_number: row.get(2)?,
                heading: row.get(3)?,
                content: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

#[tauri::command]
pub fn read_document_source(
    app: AppHandle,
    database: State<'_, Database>,
    document_id: i64,
) -> Result<DocumentSource, String> {
    let (name, mime_type, stored_path, page_count): (String, String, String, i64) = {
        let connection = database
            .0
            .lock()
            .map_err(|_| "课堂数据库锁已损坏".to_string())?;
        connection
            .query_row(
                "SELECT name, mime_type, stored_path, page_count
                 FROM course_documents WHERE id = ?1",
                [document_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "没有找到这份 Slides".to_string())?
    };

    let document_root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("documents")
        .canonicalize()
        .map_err(|error| format!("无法读取 Slides 目录：{error}"))?;
    let source_path = PathBuf::from(stored_path)
        .canonicalize()
        .map_err(|error| format!("无法找到 Slides 原文件：{error}"))?;
    if !source_path.starts_with(&document_root) {
        return Err("Slides 文件路径不在应用数据目录中".to_string());
    }

    let bytes = fs::read(&source_path).map_err(|error| format!("无法读取 Slides：{error}"))?;
    if bytes.is_empty() || bytes.len() > 50 * 1024 * 1024 {
        return Err("Slides 原文件为空或超过 50MB".to_string());
    }
    Ok(DocumentSource {
        document_id,
        name,
        mime_type,
        page_count,
        data_base64: BASE64.encode(bytes),
    })
}

#[tauri::command]
pub fn delete_lecture_document(
    app: AppHandle,
    database: State<'_, Database>,
    document_id: i64,
) -> Result<(), String> {
    let connection = database
        .0
        .lock()
        .map_err(|_| "课堂数据库锁已损坏".to_string())?;
    let path: Option<String> = connection
        .query_row(
            "SELECT stored_path FROM course_documents WHERE id = ?1",
            [document_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let safe_path = if let Some(path) = path {
        let path = PathBuf::from(path);
        if path.exists() {
            let document_root = app
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?
                .join("documents")
                .canonicalize()
                .map_err(|error| format!("无法读取 Slides 目录：{error}"))?;
            let path = path
                .canonicalize()
                .map_err(|error| format!("无法读取 Slides 文件：{error}"))?;
            if !path.starts_with(document_root) {
                return Err("Slides 文件路径不在应用数据目录中".to_string());
            }
            Some(path)
        } else {
            None
        }
    } else {
        None
    };
    let changed = connection
        .execute("DELETE FROM course_documents WHERE id = ?1", [document_id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("没有找到这份 Slides".to_string());
    }
    if let Some(path) = safe_path {
        fs::remove_file(path).map_err(|error| format!("无法删除 Slides 原文件：{error}"))?;
    }
    Ok(())
}

fn is_stop_word(word: &str) -> bool {
    matches!(
        word,
        "about"
            | "after"
            | "again"
            | "against"
            | "all"
            | "also"
            | "and"
            | "another"
            | "any"
            | "are"
            | "because"
            | "been"
            | "before"
            | "being"
            | "between"
            | "both"
            | "but"
            | "can"
            | "could"
            | "did"
            | "does"
            | "doing"
            | "each"
            | "everything"
            | "first"
            | "for"
            | "from"
            | "had"
            | "has"
            | "have"
            | "having"
            | "here"
            | "how"
            | "into"
            | "its"
            | "just"
            | "last"
            | "like"
            | "many"
            | "may"
            | "might"
            | "more"
            | "most"
            | "much"
            | "must"
            | "not"
            | "now"
            | "okay"
            | "once"
            | "only"
            | "other"
            | "our"
            | "out"
            | "over"
            | "really"
            | "right"
            | "said"
            | "same"
            | "second"
            | "should"
            | "some"
            | "something"
            | "still"
            | "such"
            | "than"
            | "that"
            | "the"
            | "their"
            | "them"
            | "then"
            | "there"
            | "these"
            | "they"
            | "thing"
            | "things"
            | "think"
            | "this"
            | "those"
            | "through"
            | "today"
            | "too"
            | "under"
            | "very"
            | "want"
            | "was"
            | "well"
            | "were"
            | "what"
            | "when"
            | "where"
            | "which"
            | "while"
            | "who"
            | "why"
            | "will"
            | "with"
            | "would"
            | "you"
            | "your"
    )
}

fn normalize_word(value: &str) -> Option<String> {
    let mut word = value.to_ascii_lowercase();
    if word.chars().count() < 3 || is_stop_word(&word) {
        return None;
    }
    if word.is_ascii() {
        let mut trim_doubled_ending = false;
        if word.len() > 4
            && word.ends_with('s')
            && !word.ends_with("ss")
            && !word.ends_with("is")
            && !word.ends_with("us")
        {
            word.truncate(word.len() - 1);
        }
        if word.len() > 6 && word.ends_with("ing") {
            word.truncate(word.len() - 3);
            trim_doubled_ending = true;
        } else if word.len() > 5 && word.ends_with("ied") {
            word.truncate(word.len() - 3);
            word.push('y');
        } else if (word.len() > 5 && word.ends_with("ed"))
            || (word.len() > 7 && word.ends_with("er"))
        {
            word.truncate(word.len() - 2);
            trim_doubled_ending = true;
        }
        let bytes = word.as_bytes();
        if trim_doubled_ending
            && bytes.len() > 3
            && bytes[bytes.len() - 1] == bytes[bytes.len() - 2]
        {
            word.truncate(word.len() - 1);
        }
    }
    (!is_stop_word(&word) && word.len() >= 3).then_some(word)
}

fn words(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(normalize_word)
        .collect()
}

fn contains_phrase(words: &[String], phrase: &[String]) -> bool {
    phrase.len() <= words.len() && words.windows(phrase.len()).any(|window| window == phrase)
}

fn continuity_bonus(
    candidate: &SlideCandidate,
    current_document_id: Option<i64>,
    current_page: Option<i64>,
) -> f64 {
    let (Some(document_id), Some(page)) = (current_document_id, current_page) else {
        return 0.0;
    };
    if candidate.document_id != document_id {
        return -0.8;
    }
    match candidate.page_number - page {
        0 => 1.6,
        1 => 1.2,
        2 => 0.6,
        3 => 0.2,
        -1 => 0.1,
        delta if delta < -1 => -((-delta - 1) as f64 * 0.2).min(1.8),
        delta if delta > 6 => -((delta - 6) as f64 * 0.12).min(2.4),
        _ => 0.0,
    }
}

fn movement_threshold(
    candidate: &SlideCandidate,
    current_document_id: Option<i64>,
    current_page: Option<i64>,
    navigation_cue: bool,
    backward_navigation_cue: bool,
) -> f64 {
    let (Some(document_id), Some(page)) = (current_document_id, current_page) else {
        return 0.55;
    };
    if candidate.document_id != document_id {
        return 0.9;
    }
    let threshold: f64 = match candidate.page_number - page {
        0 => 0.3,
        1 => {
            if navigation_cue {
                0.4
            } else {
                0.48
            }
        }
        2 => {
            if navigation_cue {
                0.5
            } else {
                0.75
            }
        }
        3 => {
            if navigation_cue {
                0.55
            } else {
                0.82
            }
        }
        4..=5 => {
            if navigation_cue {
                0.65
            } else {
                0.86
            }
        }
        delta if delta < 0 => {
            if backward_navigation_cue {
                0.62
            } else {
                1.01
            }
        }
        _ => {
            if navigation_cue {
                0.78
            } else {
                0.9
            }
        }
    };
    let generic_heading = matches!(
        candidate.heading.trim().to_ascii_lowercase().as_str(),
        "example" | "overview" | "recap" | "summary"
    );
    if generic_heading && candidate.page_number != page && !navigation_cue {
        threshold.max(0.7)
    } else {
        threshold
    }
}

fn has_navigation_cue(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "back to",
        "come back",
        "come straight to",
        "go back",
        "go to the next",
        "let us start",
        "let us talk",
        "let us review",
        "let's start",
        "let's talk",
        "let's review",
        "move on",
        "move to",
        "moving on",
        "new topic",
        "next page",
        "next part",
        "next slide",
        "now let us",
        "now let's",
        "previous page",
        "previous slide",
        "earlier page",
        "earlier slide",
        "second part",
        "second set",
        "start with",
        "turn to",
    ]
    .iter()
    .any(|cue| value.contains(cue))
}

fn has_backward_navigation_cue(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "back to the slide",
        "back to this slide",
        "back to the page",
        "back to this page",
        "go back one slide",
        "go back one page",
        "go back to the slide",
        "go back to the page",
        "previous page",
        "previous slide",
        "earlier page",
        "earlier slide",
        "return to the slide",
        "return to the page",
    ]
    .iter()
    .any(|cue| value.contains(cue))
}

fn best_slide_match(
    candidates: &[SlideCandidate],
    query: &str,
    current_document_id: Option<i64>,
    current_page: Option<i64>,
    navigation_cue: bool,
    backward_navigation_cue: bool,
) -> Option<SlideMatch> {
    let query_words = words(query);
    let query_unique = query_words.iter().cloned().collect::<HashSet<_>>();
    if query_unique.len() < 2 || candidates.is_empty() {
        return None;
    }

    let page_words = candidates
        .iter()
        .map(|candidate| words(&format!("{}\n{}", candidate.heading, candidate.content)))
        .collect::<Vec<_>>();
    let heading_words = candidates
        .iter()
        .map(|candidate| {
            words(&candidate.heading)
                .into_iter()
                .collect::<HashSet<_>>()
        })
        .collect::<Vec<_>>();
    let average_length =
        page_words.iter().map(Vec::len).sum::<usize>().max(1) as f64 / page_words.len() as f64;

    let mut document_frequency = HashMap::<String, usize>::new();
    for terms in &page_words {
        for term in terms.iter().cloned().collect::<HashSet<_>>() {
            *document_frequency.entry(term).or_default() += 1;
        }
    }
    let query_counts =
        query_words
            .iter()
            .fold(HashMap::<String, usize>::new(), |mut counts, term| {
                *counts.entry(term.clone()).or_default() += 1;
                counts
            });
    let query_phrases = (2..=3)
        .flat_map(|size| query_words.windows(size).map(Vec::from).collect::<Vec<_>>())
        .collect::<HashSet<_>>();
    let phrase_frequencies = query_phrases
        .iter()
        .map(|phrase| {
            (
                phrase.clone(),
                page_words
                    .iter()
                    .filter(|terms| contains_phrase(terms, phrase))
                    .count(),
            )
        })
        .collect::<HashMap<_, _>>();
    let page_count = candidates.len() as f64;

    let mut ranked = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let term_counts = page_words[index].iter().fold(
                HashMap::<String, usize>::new(),
                |mut counts, term| {
                    *counts.entry(term.clone()).or_default() += 1;
                    counts
                },
            );
            let mut evidence = 0.0;
            let mut matched_weight = 0.0;
            let mut available_weight = 0.0;
            let mut matched_terms = 0usize;
            for (term, query_count) in &query_counts {
                let Some(frequency) = document_frequency.get(term) else {
                    continue;
                };
                let idf =
                    (1.0 + (page_count - *frequency as f64 + 0.5) / (*frequency as f64 + 0.5)).ln();
                available_weight += idf;
                let term_frequency = *term_counts.get(term).unwrap_or(&0) as f64;
                if term_frequency == 0.0 {
                    continue;
                }
                matched_terms += 1;
                matched_weight += idf;
                let length_factor =
                    1.2 * (0.25 + 0.75 * page_words[index].len() as f64 / average_length);
                let bm25 = idf * term_frequency * 2.2 / (term_frequency + length_factor);
                let query_boost = 1.0 + (*query_count as f64).ln() * 0.2;
                let heading_boost = if heading_words[index].contains(term) {
                    idf * 0.55
                } else {
                    0.0
                };
                evidence += bm25 * query_boost + heading_boost;
            }
            if matched_terms < 2 || evidence < 1.8 {
                return None;
            }

            let mut phrase_score = 0.0;
            for phrase in &query_phrases {
                if !contains_phrase(&page_words[index], phrase) {
                    continue;
                }
                let phrase_frequency = *phrase_frequencies.get(phrase).unwrap_or(&0) as f64;
                let idf =
                    (1.0 + (page_count - phrase_frequency + 0.5) / (phrase_frequency + 0.5)).ln();
                phrase_score += idf * if phrase.len() == 3 { 1.15 } else { 0.7 };
            }
            let coverage = if available_weight > 0.0 {
                matched_weight / available_weight
            } else {
                0.0
            };
            let total = evidence
                + phrase_score
                + continuity_bonus(candidate, current_document_id, current_page);
            Some(RankedSlide {
                candidate,
                total,
                evidence,
                phrase_score,
                coverage,
                matched_terms,
            })
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.total.total_cmp(&left.total));
    let selected = ranked.iter().enumerate().find_map(|(index, item)| {
        let competing_score = if index == 0 {
            ranked.get(1).map(|other| other.total).unwrap_or(0.0)
        } else {
            ranked.first().map(|other| other.total).unwrap_or(0.0)
        };
        let confidence = item.confidence(competing_score);
        (confidence
            >= movement_threshold(
                item.candidate,
                current_document_id,
                current_page,
                navigation_cue,
                backward_navigation_cue,
            ))
        .then_some((item, confidence))
    })?;
    let (selected, confidence) = selected;
    if current_page.is_none()
        && ranked.len() > 1
        && ((selected.total - ranked[1].total) / selected.total.max(1.0)) < 0.06
    {
        return None;
    }
    Some(SlideMatch {
        document_id: selected.candidate.document_id,
        document_name: selected.candidate.document_name.clone(),
        page_number: selected.candidate.page_number,
        heading: selected.candidate.heading.clone(),
        excerpt: selected.candidate.content.chars().take(260).collect(),
        score: (selected.total * 100.0).round() as i64,
        confidence,
    })
}

#[tauri::command]
pub fn match_lecture_slide(
    database: State<'_, Database>,
    lecture_id: i64,
    query: String,
    latest_text: Option<String>,
    current_document_id: Option<i64>,
    current_page: Option<i64>,
) -> Result<Option<SlideMatch>, String> {
    let connection = database
        .0
        .lock()
        .map_err(|_| "课堂数据库锁已损坏".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT c.document_id, d.name, c.page_number, c.heading, c.content
             FROM document_chunks c JOIN course_documents d ON d.id = c.document_id
             WHERE d.lecture_id = ?1",
        )
        .map_err(|error| error.to_string())?;
    let candidates = statement
        .query_map([lecture_id], |row| {
            Ok(SlideCandidate {
                document_id: row.get(0)?,
                document_name: row.get(1)?,
                page_number: row.get(2)?,
                heading: row.get(3)?,
                content: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let latest_text = latest_text.as_deref().unwrap_or_default();
    let scoring_query = if latest_text.trim().is_empty() {
        query
    } else {
        format!("{query}\n{latest_text}\n{latest_text}")
    };
    Ok(best_slide_match(
        &candidates,
        &scoring_query,
        current_document_id,
        current_page,
        has_navigation_cue(latest_text),
        has_backward_navigation_cue(latest_text),
    ))
}

fn clean_asr_heading(value: &str) -> Option<String> {
    let heading = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|character: char| !character.is_alphanumeric())
        .to_string();
    if heading.is_empty() || !heading.chars().any(char::is_alphabetic) {
        return None;
    }
    let lower = heading.to_lowercase();
    if matches!(
        lower.as_str(),
        "overview"
            | "recap"
            | "summary"
            | "example"
            | "second example"
            | "assumptions"
            | "tutorial"
            | "radio"
    ) || lower.starts_with("tutorial: example")
    {
        return None;
    }
    if heading.is_ascii() && heading.split_whitespace().count() > 7 {
        return None;
    }
    let letters = heading
        .chars()
        .filter(|character| character.is_alphabetic());
    if !heading.contains(' ')
        && letters.clone().all(|character| character.is_uppercase())
        && letters.count() > 5
    {
        return None;
    }
    (heading.chars().count() <= 80).then_some(heading)
}

fn asr_acronyms(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .filter_map(|raw| {
            let candidate = raw.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && !matches!(character, '/' | '-' | '+')
            });
            let uppercase = candidate
                .chars()
                .filter(|character| character.is_ascii_uppercase())
                .count();
            let valid = candidate.is_ascii()
                && (2..=14).contains(&candidate.len())
                && uppercase >= 2
                && candidate
                    .chars()
                    .any(|character| character.is_ascii_alphabetic());
            valid.then(|| candidate.to_string())
        })
        .collect()
}

fn document_keyword_candidates(heading: &str, content: &str) -> Vec<String> {
    clean_asr_heading(heading)
        .into_iter()
        .chain(asr_acronyms(content))
        .collect()
}

pub fn document_keywords(connection: &rusqlite::Connection, lecture_id: i64) -> Vec<String> {
    let mut statement = match connection.prepare(
        "SELECT heading, content FROM document_chunks c
         JOIN course_documents d ON d.id = c.document_id
         WHERE d.lecture_id = ?1 ORDER BY c.page_number LIMIT 160",
    ) {
        Ok(statement) => statement,
        Err(_) => return vec![],
    };
    let Ok(rows) = statement.query_map([lecture_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) else {
        return vec![];
    };
    let mut seen = HashSet::new();
    rows.filter_map(Result::ok)
        .flat_map(|(heading, content)| document_keyword_candidates(&heading, &content))
        .filter(|keyword| seen.insert(keyword.to_lowercase()))
        .take(300)
        .collect()
}

#[tauri::command]
pub fn list_lecture_document_keywords(
    database: State<'_, Database>,
    lecture_id: i64,
) -> Result<Vec<String>, String> {
    let connection = database
        .0
        .lock()
        .map_err(|_| "课堂数据库锁已损坏".to_string())?;
    Ok(document_keywords(&connection, lecture_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_slide_phrases_and_acronyms_for_asr() {
        let keywords = document_keyword_candidates(
            "Multiple Access Schemes",
            "Base transceiver station (BTS), Mobile Switching Center (MSC), and GGSN.",
        );
        assert_eq!(keywords[0], "Multiple Access Schemes");
        assert!(keywords.contains(&"BTS".to_string()));
        assert!(keywords.contains(&"MSC".to_string()));
        assert!(keywords.contains(&"GGSN".to_string()));
        assert!(document_keyword_candidates("Overview", "").is_empty());
    }

    fn candidate(page_number: i64, heading: &str, content: &str) -> SlideCandidate {
        SlideCandidate {
            document_id: 1,
            document_name: "lecture.pdf".to_string(),
            page_number,
            heading: heading.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn reads_slide_text_in_display_order() {
        let xml = br#"<p:sld xmlns:p="p" xmlns:a="a"><a:t>Swarm intelligence</a:t><a:t>Practical example</a:t></p:sld>"#;
        assert_eq!(
            parse_slide_xml(xml),
            "Swarm intelligence\nPractical example"
        );
    }

    #[test]
    fn orders_pptx_slide_names_numerically() {
        assert!(slide_number("ppt/slides/slide2.xml") < slide_number("ppt/slides/slide10.xml"));
    }

    #[test]
    fn accepts_preparsed_legacy_ppt_pages() {
        let mut bytes = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1".to_vec();
        bytes.extend_from_slice(b"legacy-ppt");
        let pages = vec!["  First slide\n\nTopic  ".to_string(), String::new()];
        assert_eq!(
            parse_legacy_ppt(&bytes, Some(&pages)).expect("valid legacy PPT pages"),
            vec!["First slide\nTopic".to_string(), String::new()]
        );
    }

    #[test]
    fn rejects_renamed_non_ppt_files() {
        let pages = vec!["Slide".to_string()];
        assert!(parse_legacy_ppt(b"not-a-ppt", Some(&pages)).is_err());
    }

    #[test]
    fn uses_first_non_empty_line_as_heading() {
        assert_eq!(
            heading_for("\n  Topic boundary  \nDetails", 3),
            "Topic boundary"
        );
        assert_eq!(heading_for("", 3), "第 3 页");
    }

    #[test]
    fn removes_conversational_words_and_normalizes_inflections() {
        assert_eq!(
            words("So this is about base stations switching calls and planning networks"),
            vec!["base", "station", "switch", "call", "plan", "network"]
        );
        assert_eq!(words("controllers controlled"), vec!["control", "control"]);
    }

    #[test]
    fn recognizes_explicit_slide_navigation_language() {
        assert!(has_navigation_cue("Now let's move to the second part."));
        assert!(has_navigation_cue("If I go back to my slides here."));
        assert!(has_navigation_cue("Let us review the capacity model."));
        assert!(has_backward_navigation_cue(
            "If I go back to the previous slide here."
        ));
        assert!(!has_backward_navigation_cue(
            "Now let's move to the second part."
        ));
        assert!(!has_backward_navigation_cue(
            "When we come back to this particular figure."
        ));
        assert!(!has_navigation_cue(
            "The second user needs another time slot."
        ));
    }

    #[test]
    fn uses_page_continuity_to_disambiguate_repeated_overviews() {
        let candidates = vec![
            candidate(
                2,
                "Overview",
                "Recap cellular network capacity traffic modeling queueing models",
            ),
            candidate(3, "Cellular Network", "Base stations cells and coverage"),
            candidate(
                4,
                "Architecture",
                "Controller switching center architecture",
            ),
            candidate(
                5,
                "Data Network",
                "Packet switching public internet gateway",
            ),
            candidate(6, "Planning", "Radio transmission and core planning"),
            candidate(7, "Radio Planning", "Coverage quality and design criteria"),
            candidate(
                8,
                "Overview",
                "Recap cellular network capacity traffic modeling queueing models",
            ),
            candidate(9, "Multiplexing", "Packets buffers queue delay and loss"),
            candidate(
                14,
                "Overview",
                "Recap cellular network capacity traffic modeling queueing models",
            ),
        ];
        assert!(best_slide_match(
            &candidates,
            "Let us review cellular network capacity and traffic modeling before queueing models",
            None,
            None,
            false,
            false,
        )
        .is_none());
        let query =
            "Let us review cellular network capacity and traffic modeling before queueing models";
        let matched = best_slide_match(
            &candidates,
            query,
            Some(1),
            Some(7),
            has_navigation_cue(query),
            has_backward_navigation_cue(query),
        )
        .expect("the nearest next overview should be selected");
        assert_eq!(matched.page_number, 8);
    }

    #[test]
    fn prefers_specific_architecture_terms_over_generic_network_text() {
        let candidates = vec![
            candidate(3, "Cellular Network", "Base stations transmit data and each cell provides network coverage"),
            candidate(4, "Network Architecture", "Base transceiver station BTS base station controller BSC mobile switching center MSC"),
            candidate(7, "Radio Network Planning", "Provide a cost effective radio network with coverage capacity and quality"),
        ];
        let matched = best_slide_match(
            &candidates,
            "A number of base stations are controlled by a base station controller which is controlled by a mobile switching center",
            Some(1),
            Some(3),
            false,
            false,
        )
        .expect("architecture evidence should produce a match");
        assert_eq!(matched.page_number, 4);
        assert!(matched.confidence >= 0.3);
    }

    #[test]
    fn strong_evidence_can_override_a_large_forward_jump() {
        let candidates = vec![
            candidate(3, "Cellular Network", "Base stations cells and coverage"),
            candidate(7, "Radio Network Planning", "Coverage capacity and network quality"),
            candidate(12, "Capacity Design Principles", "GSM available bandwidth 5 MHz guard band channel spacing 200 KHz TDMA 8 slots 192 calls"),
        ];
        let query = "For GSM the available bandwidth is five megahertz with two hundred kilohertz channel spacing and eight TDMA slots";
        let unrestricted = best_slide_match(&candidates, query, None, None, false, false)
            .expect("specific capacity evidence should be independently strong");
        assert!(
            unrestricted.confidence >= 0.82,
            "unexpected confidence: {}",
            unrestricted.confidence
        );
        let matched = best_slide_match(&candidates, query, Some(1), Some(3), true, false)
            .expect("specific capacity evidence should override continuity");
        assert_eq!(matched.page_number, 12);
    }

    #[test]
    fn keeps_the_current_page_while_the_teacher_is_still_explaining_it() {
        let candidates = vec![
            candidate(
                7,
                "Radio Network Planning",
                "Provide a cost effective radio network in terms of coverage capacity and quality",
            ),
            candidate(
                8,
                "Overview",
                "Cellular network capacity multiple access traffic modeling queueing models",
            ),
            candidate(9, "Multiplexing", "Packets buffers queue delay and loss"),
        ];
        let matched = best_slide_match(
            &candidates,
            "Radio network planning must provide enough coverage capacity and quality",
            Some(1),
            Some(7),
            false,
            false,
        )
        .expect("the current page has direct evidence");
        assert_eq!(matched.page_number, 7);
    }

    #[test]
    fn explicit_navigation_can_return_to_an_earlier_slide() {
        let candidates = vec![
            candidate(
                6,
                "Erlang B Formula",
                "Blocking probability offered traffic no waiting queue lost calls cleared",
            ),
            candidate(
                7,
                "Erlang C Formula",
                "Waiting probability queued calls delayed traffic service time",
            ),
        ];
        let query = "Let us go back to the previous slide about Erlang B blocking probability and lost calls cleared";
        let matched = best_slide_match(
            &candidates,
            query,
            Some(1),
            Some(7),
            has_navigation_cue(query),
            has_backward_navigation_cue(query),
        )
        .expect("an explicit back-navigation cue should allow the previous slide");
        assert_eq!(matched.page_number, 6);

        let ordinary_transition =
            "Now let's move on to Erlang B blocking probability and lost calls cleared";
        assert!(best_slide_match(
            &candidates,
            ordinary_transition,
            Some(1),
            Some(7),
            has_navigation_cue(ordinary_transition),
            has_backward_navigation_cue(ordinary_transition),
        )
        .is_none());
    }

    #[test]
    #[ignore = "requires SLIDE_REPLAY_DB and SLIDE_REPLAY_LECTURE"]
    fn replays_a_saved_lecture_for_match_diagnostics() {
        let database_path = std::env::var("SLIDE_REPLAY_DB").expect("SLIDE_REPLAY_DB is required");
        let lecture_id = std::env::var("SLIDE_REPLAY_LECTURE")
            .expect("SLIDE_REPLAY_LECTURE is required")
            .parse::<i64>()
            .expect("lecture id must be an integer");
        let connection = rusqlite::Connection::open(database_path).expect("database should open");
        let mut slide_statement = connection
            .prepare(
                "SELECT c.document_id, d.name, c.page_number, c.heading, c.content
                 FROM document_chunks c JOIN course_documents d ON d.id = c.document_id
                 WHERE d.lecture_id = ?1 ORDER BY c.page_number",
            )
            .unwrap();
        let candidates = slide_statement
            .query_map([lecture_id], |row| {
                Ok(SlideCandidate {
                    document_id: row.get(0)?,
                    document_name: row.get(1)?,
                    page_number: row.get(2)?,
                    heading: row.get(3)?,
                    content: row.get(4)?,
                })
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut transcript_statement = connection
            .prepare(
                "SELECT start_ms, english FROM transcript_segments
                 WHERE lecture_id = ?1 ORDER BY seq",
            )
            .unwrap();
        let segments = transcript_statement
            .query_map([lecture_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        let mut current: Option<SlideMatch> = None;
        let mut pending: Option<(i64, usize)> = None;
        for (index, (start_ms, _)) in segments.iter().enumerate() {
            let query = segments[..=index]
                .iter()
                .rev()
                .take_while(|(candidate_ms, _)| start_ms - candidate_ms <= 35_000)
                .take(16)
                .map(|(_, text)| text.as_str())
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" ");
            let navigation_text = segments[..=index]
                .iter()
                .rev()
                .take(3)
                .map(|(_, text)| text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let scoring_query = format!("{query}\n{navigation_text}\n{navigation_text}");
            let candidate = best_slide_match(
                &candidates,
                &scoring_query,
                current.as_ref().map(|matched| matched.document_id),
                current.as_ref().map(|matched| matched.page_number),
                has_navigation_cue(&navigation_text),
                has_backward_navigation_cue(&navigation_text),
            );
            let Some(candidate) = candidate else {
                pending = None;
                continue;
            };
            if current.as_ref().map(|matched| matched.page_number) == Some(candidate.page_number) {
                current = Some(candidate);
                pending = None;
                continue;
            }
            let confirmations = pending
                .filter(|(page, _)| *page == candidate.page_number)
                .map(|(_, count)| count + 1)
                .unwrap_or(1);
            pending = Some((candidate.page_number, confirmations));
            let page_delta = current
                .as_ref()
                .filter(|matched| matched.document_id == candidate.document_id)
                .map(|matched| candidate.page_number - matched.page_number);
            let required_confirmations = if page_delta.is_none() || page_delta == Some(1) {
                2
            } else {
                3
            };
            if candidate.confidence >= 0.9 || confirmations >= required_confirmations {
                println!(
                    "{:02}:{:02} -> P.{} {:0.0}% {}",
                    start_ms / 60_000,
                    start_ms / 1_000 % 60,
                    candidate.page_number,
                    candidate.confidence * 100.0,
                    candidate.heading
                );
                current = Some(candidate);
                pending = None;
            }
        }
        assert!(!candidates.is_empty());
        assert!(current.is_some());
    }
}
