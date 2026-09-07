#![recursion_limit = "256"]

use futures_util::StreamExt;
use keyring::Entry;
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{error::Error as StdError, sync::OnceLock, time::Duration};
use tauri::{AppHandle, Emitter, Manager};

mod alibaba_asr;
mod archive;
mod chat;
mod deepgram_asr;
mod documents;
mod storage;
mod summary;

use alibaba_asr::{finish_alibaba_asr, send_alibaba_asr_audio, start_alibaba_asr, AlibabaAsrState};
use archive::{
    apply_pending_restore, create_app_backup, export_lecture_markdown, stage_app_restore,
};
use chat::{ask_lecture_chat, get_chat_history};
use deepgram_asr::{
    finish_deepgram_asr, send_deepgram_asr_audio, start_deepgram_asr, DeepgramAsrState,
};
use documents::{
    delete_lecture_document, import_lecture_slides, list_document_chunks,
    list_lecture_document_keywords, list_lecture_documents, match_lecture_slide,
    read_document_source,
};

use storage::{
    add_lecture_bookmark, begin_lecture, delete_lecture, delete_lecture_bookmark, finish_lecture,
    get_lecture, get_recoverable_lecture, list_courses, list_glossary_terms,
    list_lecture_bookmarks, list_lectures, replace_glossary_terms, save_course,
    save_lecture_snapshot, save_transcript_translation, update_lecture_bookmark, Database,
};
use summary::generate_topic_summary;

const KEYRING_SERVICE: &str = "com.nus.lecture-assistant";
const OPENAI_KEYRING_USER: &str = "openai-api-key";
const OPENAI_TRANSLATION_KEYRING_USER: &str = "openai-translation-api-key";
const OPENAI_SUMMARY_KEYRING_USER: &str = "openai-summary-api-key";
const OPENAI_LECTURE_KEYRING_USER: &str = "openai-lecture-summary-api-key";
const ALIBABA_KEYRING_USER: &str = "alibaba-api-key";
const ALIBABA_ASR_KEYRING_USER: &str = "alibaba-asr-api-key";
const DEEPGRAM_KEYRING_USER: &str = "deepgram-api-key";
const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const ALIBABA_BASE_URL: &str = "https://dashscope-intl.aliyuncs.com/compatible-mode/v1";
const OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";
const OLLAMA_CLOUD_BASE_URL: &str = "https://ollama.com";
pub(crate) const OLLAMA_CONTEXT_TOKENS: usize = 16_384;
pub(crate) const OLLAMA_OUTPUT_TOKENS: usize = 4_096;
const OLLAMA_INPUT_BYTES: usize = 11_000;
static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static TRANSLATION_GATE: OnceLock<tokio::sync::Semaphore> = OnceLock::new();

pub(crate) fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(45))
        .tcp_keepalive(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub(crate) fn http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(build_http_client)
}

fn translation_gate() -> &'static tokio::sync::Semaphore {
    TRANSLATION_GATE.get_or_init(|| tokio::sync::Semaphore::new(2))
}

/// Translation requests are short-lived and safe to retry when the provider is
/// temporarily throttling us or returns a transient server error. Keeping this
/// decision in one helper makes provider changes easier to test and avoids
/// retrying permanent authentication/model errors.
fn is_retryable_translation_status(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::REQUEST_TIMEOUT
            | reqwest::StatusCode::TOO_MANY_REQUESTS
            | reqwest::StatusCode::INTERNAL_SERVER_ERROR
            | reqwest::StatusCode::BAD_GATEWAY
            | reqwest::StatusCode::SERVICE_UNAVAILABLE
            | reqwest::StatusCode::GATEWAY_TIMEOUT
    )
}

fn translation_retry_delay(retry_after: Option<&str>, retry_count: usize) -> Option<Duration> {
    match retry_after {
        Some(value) => value
            .trim()
            .parse::<u64>()
            .ok()
            // Do not occupy a realtime queue slot for a long provider cooldown.
            // Unsupported HTTP-date values also return the provider's error.
            .filter(|seconds| *seconds <= 5)
            .map(Duration::from_secs),
        None => Some(translation_retry_backoff(retry_count)),
    }
}

fn translation_retry_backoff(retry_count: usize) -> Duration {
    // Small exponential backoff, capped below one second for the realtime
    // translation path.
    let milliseconds = 200_u64.saturating_mul(2_u64.pow(retry_count.min(3) as u32));
    Duration::from_millis(milliseconds.min(900))
}

fn clip_characters(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn provider_connection_error(
    provider_label: &str,
    operation: &str,
    error: &reqwest::Error,
    retry_count: usize,
    shared_alibaba_route: bool,
) -> String {
    let category = if error.is_timeout() {
        "连接超时"
    } else if error.is_connect() {
        "网络连接失败"
    } else if error.is_request() {
        "请求发送失败"
    } else {
        "响应读取失败"
    };
    let mut causes = Vec::new();
    let mut source = StdError::source(error);
    while let Some(cause) = source {
        let detail = cause.to_string();
        if !detail.is_empty() && !causes.contains(&detail) {
            causes.push(detail);
        }
        source = cause.source();
    }
    let detail = causes.last().cloned().unwrap_or_else(|| error.to_string());
    let retry_note = if retry_count == 0 {
        String::new()
    } else {
        format!("，已重试 {retry_count} 次")
    };
    let route_note = if shared_alibaba_route {
        "；当前使用阿里云共享线路，请在设置中填写 llm- 开头的业务空间 ID"
    } else {
        ""
    };
    format!(
        "无法连接{provider_label}{operation}服务：{category}（{detail}{retry_note}）{route_note}"
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RealtimeConfig {
    course_name: String,
    course_context: String,
    keywords: Vec<String>,
    transcription_model: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationRequest {
    segment_id: String,
    english: String,
    previous_english: Option<String>,
    course_name: String,
    glossary: Vec<String>,
    provider: String,
    workspace_id: String,
    /// Optional OpenAI-compatible API base URL. Empty keeps the official endpoint.
    #[serde(default)]
    openai_base_url: String,
    #[serde(default)]
    ollama_base_url: String,
    /// OpenAI key slot: translation, summary, or lecture.
    #[serde(default)]
    openai_key_slot: String,
    model: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranslationEvent {
    segment_id: String,
    kind: String,
    text: String,
}

fn transcription_prompt(config: &RealtimeConfig) -> String {
    let mut vocabulary = String::new();
    for keyword in &config.keywords {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            continue;
        }
        let separator = if vocabulary.is_empty() { "" } else { ", " };
        if vocabulary.chars().count() + separator.len() + keyword.chars().count() > 1_200 {
            break;
        }
        vocabulary.push_str(separator);
        vocabulary.push_str(keyword);
    }
    let vocabulary_note = if vocabulary.is_empty() {
        String::new()
    } else {
        format!(" Spell this course vocabulary exactly when spoken: {vocabulary}.")
    };
    format!(
        "A university lecture in English for the NUS course '{}'. {}{} Transcribe faithfully in English. Preserve technical terms, names, symbols, numbers, and formulas.",
        clip_characters(config.course_name.trim(), 200),
        clip_characters(config.course_context.trim(), 4_000),
        vocabulary_note
    )
}

fn provider_details(provider: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    match provider {
        "openai" => Ok(("OpenAI", OPENAI_BASE_URL, OPENAI_KEYRING_USER)),
        "openai-translation" => Ok((
            "OpenAI 实时翻译",
            OPENAI_BASE_URL,
            OPENAI_TRANSLATION_KEYRING_USER,
        )),
        "openai-summary" => Ok((
            "OpenAI 阶段总结",
            OPENAI_BASE_URL,
            OPENAI_SUMMARY_KEYRING_USER,
        )),
        "openai-lecture" => Ok((
            "OpenAI 整课总结",
            OPENAI_BASE_URL,
            OPENAI_LECTURE_KEYRING_USER,
        )),
        "alibaba" => Ok(("阿里云百炼", ALIBABA_BASE_URL, ALIBABA_KEYRING_USER)),
        "alibaba-asr" => Ok(("阿里云实时转写", "", ALIBABA_ASR_KEYRING_USER)),
        "deepgram" => Ok(("Deepgram", "", DEEPGRAM_KEYRING_USER)),
        "ollama" => Ok(("Ollama 本地模型", OLLAMA_BASE_URL, "ollama-local")),
        "ollama-cloud" => Ok(("Ollama 云端", OLLAMA_CLOUD_BASE_URL, "ollama-cloud-api-key")),
        _ => Err("不支持的模型供应商".to_string()),
    }
}

fn alibaba_workspace_host(workspace_id: &str) -> Result<String, String> {
    let workspace_id = workspace_id.trim();
    if workspace_id.is_empty() {
        return Ok("dashscope-intl.aliyuncs.com".to_string());
    }
    let valid = workspace_id.starts_with("llm-")
        && workspace_id.len() <= 80
        && workspace_id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        });
    if !valid {
        return Err("阿里云业务空间 ID 格式不正确，应类似 llm-xxxxxxxx".to_string());
    }
    Ok(format!("{workspace_id}.ap-southeast-1.maas.aliyuncs.com"))
}

#[allow(dead_code)]
pub(crate) fn provider_base_url(provider: &str, workspace_id: &str) -> Result<String, String> {
    provider_base_url_with_openai(provider, workspace_id, "")
}

/// Resolve a provider's text API endpoint, allowing OpenAI-compatible services
/// to override the official OpenAI base URL. Realtime WebRTC remains pinned to
/// the official endpoint (see `create_realtime_call`).
pub(crate) fn provider_base_url_with_openai(
    provider: &str,
    workspace_id: &str,
    openai_base_url: &str,
) -> Result<String, String> {
    match provider {
        "openai" => normalize_openai_base_url(openai_base_url),
        "alibaba" => Ok(format!(
            "https://{}/compatible-mode/v1",
            alibaba_workspace_host(workspace_id)?
        )),
        _ => Err("不支持的模型供应商".to_string()),
    }
}

pub(crate) fn text_provider_base_url(
    provider: &str,
    workspace_id: &str,
    openai_base_url: &str,
    ollama_base_url: &str,
) -> Result<String, String> {
    if provider == "ollama" {
        normalize_ollama_base_url(ollama_base_url)
    } else {
        provider_base_url_with_openai(provider, workspace_id, openai_base_url)
    }
}

fn normalize_ollama_base_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(OLLAMA_BASE_URL.to_string());
    }
    if trimmed.chars().count() > 512 {
        return Err("Ollama 地址过长".to_string());
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        || trimmed.starts_with("http:///")
        || trimmed.starts_with("https:///")
    {
        return Err("Ollama 地址格式不正确，应为 http://host:11434".to_string());
    }
    let mut parsed = reqwest::Url::parse(trimmed)
        .map_err(|_| "Ollama 地址格式不正确，应为 http://host:11434".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("Ollama 地址必须包含 http:// 或 https:// 和有效主机".to_string());
    }
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("Ollama 地址不得包含用户名、密码、查询参数或片段".to_string());
    }
    let path = parsed.path().trim_end_matches('/');
    let normalized_path = path
        .strip_suffix("/api/chat")
        .or_else(|| path.strip_suffix("/api"))
        .or_else(|| path.strip_suffix("/v1"))
        .unwrap_or(path)
        .to_string();
    parsed.set_path(&normalized_path);
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

/// Validate and normalize a user-supplied OpenAI-compatible API base URL.
///
/// Only HTTPS URLs with a host are accepted. Query strings, fragments and
/// embedded credentials are rejected so a setting cannot unexpectedly leak
/// secrets or alter request routing. The caller supplies the complete base
/// path (normally ending in `/v1`).
pub(crate) fn normalize_openai_base_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(OPENAI_BASE_URL.to_string());
    }
    if trimmed.chars().count() > 512 {
        return Err("OpenAI Base URL 过长".to_string());
    }
    if !trimmed.starts_with("https://") || trimmed.starts_with("https:///") {
        return Err("OpenAI Base URL 格式不正确，应为 https:// 开头的完整地址".to_string());
    }
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|_| "OpenAI Base URL 格式不正确，应为 https:// 开头的完整地址".to_string())?;
    if parsed.scheme() != "https" {
        return Err("OpenAI Base URL 仅允许使用 HTTPS（https://）".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("OpenAI Base URL 必须包含有效域名".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("OpenAI Base URL 不得包含用户名或密码".to_string());
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("OpenAI Base URL 不得包含查询参数或片段".to_string());
    }
    Ok(trimmed.trim_end_matches('/').to_string())
}

fn keyring_entry(provider: &str) -> Result<Entry, String> {
    let (_, _, user) = provider_details(provider)?;
    Entry::new(KEYRING_SERVICE, user).map_err(|error| format!("无法访问系统凭据库：{error}"))
}

fn read_api_key() -> Result<String, String> {
    read_provider_api_key("openai")
}

pub(crate) fn read_provider_api_key(provider: &str) -> Result<String, String> {
    if provider == "ollama" {
        return Ok(String::new());
    }
    let (label, _, _) = provider_details(provider)?;
    match keyring_entry(provider)?.get_password() {
        Ok(key) => Ok(key),
        Err(_) if provider.starts_with("openai-") => keyring_entry("openai")?
            .get_password()
            .map_err(|_| format!("尚未保存 {label} API Key")),
        Err(_) => Err(format!("尚未保存 {label} API Key")),
    }
}

pub(crate) fn compact_error(provider: &str, body: &str, status: reqwest::StatusCode) -> String {
    let label = provider_details(provider)
        .map(|details| details.0)
        .unwrap_or("模型服务");
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.chars().take(240).collect());

    format!("{label}请求失败（{}）：{}", status.as_u16(), message)
}

#[tauri::command]
async fn has_api_key() -> bool {
    tauri::async_runtime::spawn_blocking(|| read_api_key().is_ok())
        .await
        .unwrap_or(false)
}

#[tauri::command]
async fn has_provider_api_key(provider: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || read_provider_api_key(&provider).is_ok())
        .await
        .unwrap_or(false)
}

#[tauri::command]
fn save_api_key(api_key: String) -> Result<(), String> {
    save_provider_api_key("openai".to_string(), api_key)
}

#[tauri::command]
fn save_provider_api_key(provider: String, api_key: String) -> Result<(), String> {
    let (label, _, _) = provider_details(&provider)?;
    let trimmed = api_key.trim();
    let valid_format = if provider == "deepgram" || provider == "ollama-cloud" {
        trimmed.len() >= 20
    } else if provider == "openai" || provider.starts_with("openai-") {
        // OpenAI-compatible gateways often use prefixes other than `sk-`.
        // The Base URL, rather than the key prefix, determines the route.
        trimmed.len() >= 16
    } else {
        trimmed.starts_with("sk-") && trimmed.len() >= 20
    };
    if !valid_format {
        return Err(format!("{label} API Key 格式看起来不正确"));
    }

    keyring_entry(&provider)?
        .set_password(trimmed)
        .map_err(|error| format!("无法保存到系统凭据库：{error}"))
}

#[tauri::command]
fn delete_api_key() -> Result<(), String> {
    delete_provider_api_key("openai".to_string())
}

#[tauri::command]
fn delete_provider_api_key(provider: String) -> Result<(), String> {
    match keyring_entry(&provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("无法从系统凭据库删除密钥：{error}")),
    }
}

#[tauri::command]
async fn create_realtime_call(sdp: String, config: RealtimeConfig) -> Result<String, String> {
    if sdp.trim().is_empty() {
        return Err("WebRTC 会话描述为空".to_string());
    }
    if sdp.len() > 2 * 1024 * 1024 {
        return Err("WebRTC 会话描述异常大".to_string());
    }
    if config.transcription_model.trim().is_empty()
        || config.transcription_model.chars().count() > 100
    {
        return Err("语音识别模型名称无效".to_string());
    }

    let api_key = read_api_key()?;
    let prompt = transcription_prompt(&config);
    let keywords = config
        .keywords
        .iter()
        .map(|keyword| keyword.trim())
        .filter(|keyword| !keyword.is_empty() && keyword.chars().count() <= 100)
        .take(200)
        .collect::<Vec<_>>();

    let session = json!({
        "type": "transcription",
        "audio": {
            "input": {
                "transcription": {
                    "model": config.transcription_model,
                    "prompt": prompt,
                    "keywords": keywords,
                    "languages": ["en"],
                    "delay": "low"
                },
                "turn_detection": {
                    "type": "server_vad",
                    "threshold": 0.45,
                    "prefix_padding_ms": 300,
                    "silence_duration_ms": 650
                }
            }
        }
    });

    let form = multipart::Form::new()
        .text("sdp", sdp)
        .text("session", session.to_string());

    let response = http_client()
        .post("https://api.openai.com/v1/realtime/calls")
        .bearer_auth(api_key)
        .header(
            "OpenAI-Safety-Identifier",
            "nus-lecture-assistant-local-user",
        )
        .multipart(form)
        .send()
        .await
        .map_err(|error| format!("无法连接 OpenAI Realtime：{error}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("无法读取 Realtime 响应：{error}"))?;

    if !status.is_success() {
        return Err(compact_error("openai", &body, status));
    }

    Ok(body)
}

fn emit_translation(app: &AppHandle, segment_id: &str, kind: &str, text: &str) {
    let _ = app.emit(
        "translation-event",
        TranslationEvent {
            segment_id: segment_id.to_string(),
            kind: kind.to_string(),
            text: text.to_string(),
        },
    );
}

fn translation_delta(event: &Value) -> Option<&str> {
    if event.get("type").and_then(Value::as_str) == Some("response.output_text.delta") {
        return event.get("delta").and_then(Value::as_str);
    }
    event
        .get("choices")?
        .as_array()?
        .first()?
        .get("delta")?
        .get("content")?
        .as_str()
}

#[derive(Debug, PartialEq, Eq)]
enum TranslationStreamEvent {
    Delta(String),
    DeltaAndCompleted(String),
    Completed,
    Error(String),
    Ignore,
}

fn next_sse_line(buffer: &mut Vec<u8>) -> Option<String> {
    let line_end = buffer.iter().position(|byte| *byte == b'\n')?;
    let mut bytes = buffer.drain(..=line_end).collect::<Vec<_>>();
    bytes.pop();
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn translation_stream_event(line: &str) -> TranslationStreamEvent {
    let data = line
        .strip_prefix("data:")
        .map(str::trim_start)
        .unwrap_or_else(|| line.trim());
    if data == "[DONE]" {
        return TranslationStreamEvent::Completed;
    }
    if data.is_empty() {
        return TranslationStreamEvent::Ignore;
    }
    let Ok(event) = serde_json::from_str::<Value>(data) else {
        return TranslationStreamEvent::Ignore;
    };
    let finish_reason = event
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str);
    if matches!(finish_reason, Some("length" | "content_filter")) {
        return TranslationStreamEvent::Error("翻译输出被截断，请重试".to_string());
    }
    if let Some(delta) = translation_delta(&event).filter(|delta| !delta.is_empty()) {
        return if finish_reason == Some("stop") {
            TranslationStreamEvent::DeltaAndCompleted(delta.to_string())
        } else {
            TranslationStreamEvent::Delta(delta.to_string())
        };
    }
    if finish_reason == Some("stop") {
        return TranslationStreamEvent::Completed;
    }
    match event.get("type").and_then(Value::as_str) {
        Some("response.completed") => TranslationStreamEvent::Completed,
        Some("response.incomplete" | "response.failed") => {
            TranslationStreamEvent::Error("翻译服务未完成输出，请重试".to_string())
        }
        Some("error") => TranslationStreamEvent::Error(
            event
                .pointer("/error/message")
                .or_else(|| event.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("翻译服务返回未知错误")
                .to_string(),
        ),
        _ => TranslationStreamEvent::Ignore,
    }
}

pub(crate) fn validate_ollama_response(response: &Value) -> Result<(), String> {
    if let Some(error) = response.get("error").and_then(Value::as_str) {
        return Err(format!("Ollama 返回错误：{error}"));
    }
    if response.get("done").and_then(Value::as_bool) != Some(true) {
        return Err("Ollama 响应未完整结束，请重试".to_string());
    }
    if response.get("done_reason").and_then(Value::as_str) == Some("length") {
        return Err("Ollama 输出达到长度上限，内容可能被截断；请缩短输入或更换模型".to_string());
    }
    Ok(())
}

pub(crate) fn validate_ollama_input(parts: &[&str]) -> Result<(), String> {
    // Bytes are a conservative upper bound for the byte-fallback tokenizers
    // used by the recommended Qwen models. Reserve output and chat-template
    // space within a fixed context instead of silently truncating input.
    let bytes = parts.iter().map(|part| part.len()).sum::<usize>();
    if bytes > OLLAMA_INPUT_BYTES {
        return Err(format!(
            "本地模型输入超过当前安全上限（{bytes}/{OLLAMA_INPUT_BYTES} UTF-8 字节，含提示词和资料）。请缩短阶段材料、课程背景或术语表；长整课总结建议使用云端模型"
        ));
    }
    Ok(())
}

fn ollama_translation_stream_event(line: &str) -> TranslationStreamEvent {
    let line = line.trim();
    if line.is_empty() {
        return TranslationStreamEvent::Ignore;
    }
    let event = match serde_json::from_str::<Value>(line) {
        Ok(event) => event,
        Err(_) => {
            return TranslationStreamEvent::Error("Ollama 翻译响应格式无效或被截断".to_string())
        }
    };
    if let Some(error) = event.get("error").and_then(Value::as_str) {
        return TranslationStreamEvent::Error(format!("Ollama 返回错误：{error}"));
    }
    let completed = event.get("done").and_then(Value::as_bool) == Some(true);
    if completed {
        if let Err(error) = validate_ollama_response(&event) {
            return TranslationStreamEvent::Error(error);
        }
    }
    let delta = event
        .pointer("/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match (delta.is_empty(), completed) {
        (false, false) => TranslationStreamEvent::Delta(delta.to_string()),
        (false, true) => TranslationStreamEvent::DeltaAndCompleted(delta.to_string()),
        (true, true) => TranslationStreamEvent::Completed,
        (true, false) => TranslationStreamEvent::Ignore,
    }
}

fn validate_translation_completion(received_text: bool, completed: bool) -> Result<(), String> {
    if !received_text {
        return Err("翻译服务没有返回文本".to_string());
    }
    if !completed {
        return Err("翻译连接提前结束，译文可能不完整；请点击重试".to_string());
    }
    Ok(())
}

fn qwen_mt_terms(glossary: &[String]) -> Vec<Value> {
    glossary
        .iter()
        .filter_map(|entry| {
            let (source, target_with_aliases) = entry.split_once('→')?;
            let source = source.trim();
            let target = target_with_aliases
                .split('；')
                .next()
                .unwrap_or_default()
                .trim();
            if source.is_empty() || target.is_empty() {
                return None;
            }
            let target = if target == "保留英文" {
                source
            } else {
                target
            };
            Some(json!({"source": source, "target": target}))
        })
        .take(50)
        .collect()
}

fn relevant_glossary(glossary: &[String], context: &str, max_items: usize) -> Vec<String> {
    let context = context.to_lowercase();
    let matches_context = |entry: &str| {
        let lower = entry.to_lowercase();
        let source = lower
            .split_once('→')
            .map(|(source, _)| source)
            .unwrap_or(&lower);
        if source.trim().chars().count() >= 2 && context.contains(source.trim()) {
            return true;
        }
        lower
            .split_once("别名：")
            .map(|(_, aliases)| {
                aliases
                    .split([',', '，', ';', '；'])
                    .map(str::trim)
                    .any(|alias| alias.chars().count() >= 2 && context.contains(alias))
            })
            .unwrap_or(false)
    };
    let mut selected = glossary
        .iter()
        .enumerate()
        .filter(|(_, entry)| matches_context(entry))
        .map(|(index, entry)| (index, entry.clone()))
        .take(max_items)
        .collect::<Vec<_>>();
    for (index, entry) in glossary.iter().enumerate().take(8) {
        if selected.len() >= max_items {
            break;
        }
        if !selected
            .iter()
            .any(|(selected_index, _)| *selected_index == index)
        {
            selected.push((index, entry.clone()));
        }
    }
    selected.into_iter().map(|(_, entry)| entry).collect()
}

#[tauri::command]
async fn translate_segment(app: AppHandle, request: TranslationRequest) -> Result<(), String> {
    // Keep a timed-out realtime request from occupying the bounded translation
    // queue indefinitely when a provider stops sending bytes.
    let segment_id = request.segment_id.clone();
    let timeout = Duration::from_secs(match request.provider.as_str() {
        "ollama" => 110,
        _ => 55,
    });
    match tokio::time::timeout(timeout, translate_segment_inner(app.clone(), request)).await {
        Ok(result) => result,
        Err(_) => {
            let message = if timeout.as_secs() <= 55 {
                "翻译请求超时（云端 55 秒），可稍后点击重试"
            } else {
                "翻译请求超时（本地模型 110 秒），可稍后点击重试"
            };
            emit_translation(&app, &segment_id, "error", message);
            Err(message.to_string())
        }
    }
}

fn translation_base_url(request: &TranslationRequest) -> Result<String, String> {
    if request.provider == "ollama-cloud" {
        return Ok(OLLAMA_CLOUD_BASE_URL.to_string());
    }
    text_provider_base_url(
        &request.provider,
        &request.workspace_id,
        &request.openai_base_url,
        &request.ollama_base_url,
    )
}

async fn translate_segment_inner(
    app: AppHandle,
    request: TranslationRequest,
) -> Result<(), String> {
    let english = request.english.trim();
    if english.is_empty() {
        return Err("没有可翻译的英文内容".to_string());
    }
    if english.chars().count() > 8_000 || request.segment_id.chars().count() > 200 {
        return Err("待翻译内容异常长".to_string());
    }
    if request.model.trim().is_empty() || request.model.chars().count() > 100 {
        return Err("翻译模型名称无效".to_string());
    }
    let (provider_label, _, _) = provider_details(&request.provider)?;
    // Cloud credentials must never follow a user-configurable local URL.
    let base_url = translation_base_url(&request)?;
    let _permit = translation_gate()
        .acquire()
        .await
        .map_err(|_| "翻译队列已关闭".to_string())?;
    let api_key = if request.provider == "ollama" {
        String::new()
    } else if request.provider == "openai" {
        read_provider_api_key(if request.openai_key_slot.trim().is_empty() {
            "openai"
        } else {
            match request.openai_key_slot.as_str() {
                "translation" => "openai-translation",
                "summary" => "openai-summary",
                "lecture" => "openai-lecture",
                _ => "openai",
            }
        })?
    } else {
        read_provider_api_key(&request.provider)?
    };
    let previous = clip_characters(
        request.previous_english.as_deref().unwrap_or_default(),
        2_000,
    );
    let selected_glossary = relevant_glossary(
        &request.glossary,
        &format!("{previous}\n{english}"),
        if request.model.starts_with("qwen-mt-") {
            50
        } else {
            24
        },
    );
    let glossary = selected_glossary.join("\n");
    let input = format!(
        "Course: {}\nRelevant glossary:\n{}\nPrevious sentence for context: {}\nTranslate this sentence: {}",
        clip_characters(request.course_name.trim(), 200), glossary, previous, english
    );

    let instructions = "Translate the lecturer's English into concise, natural Simplified Chinese. Preserve all technical terms, numbers, equations, negation, uncertainty, and emphasis. Use the supplied glossary when relevant. Return only the Chinese translation, with no labels or explanation.";
    let is_qwen_mt = request.provider == "alibaba" && request.model.starts_with("qwen-mt-");
    let is_ollama = matches!(request.provider.as_str(), "ollama" | "ollama-cloud");
    let (endpoint, payload) = if is_ollama {
        validate_ollama_input(&[instructions, &input])?;
        (
            format!("{base_url}/api/chat"),
            json!({
                "model": request.model,
                "messages": [
                    {"role": "system", "content": instructions},
                    {"role": "user", "content": input}
                ],
                "stream": true,
                "think": false,
                "keep_alive": "5m",
                "options": {
                    "temperature": 0.1,
                    "num_ctx": OLLAMA_CONTEXT_TOKENS,
                    "num_predict": OLLAMA_OUTPUT_TOKENS
                }
            }),
        )
    } else if is_qwen_mt {
        let mut translation_options = json!({
            "source_lang": "English",
            "target_lang": "Chinese",
            "domains": format!(
                "A university lecture for the course '{}'. Translate into concise Simplified Chinese and preserve technical terminology, numbers, equations, negation, uncertainty, and emphasis.",
                clip_characters(request.course_name.trim(), 200)
            )
        });
        let terms = qwen_mt_terms(&selected_glossary);
        if !terms.is_empty() {
            translation_options["terms"] = json!(terms);
        }
        (
            format!("{base_url}/chat/completions"),
            json!({
                "model": request.model,
                "messages": [{"role": "user", "content": english}],
                "stream": true,
                "translation_options": translation_options
            }),
        )
    } else if request.provider == "alibaba" {
        (
            format!("{base_url}/chat/completions"),
            json!({
                "model": request.model,
                "messages": [
                    {"role": "system", "content": instructions},
                    {"role": "user", "content": input}
                ],
                "stream": true,
                "enable_thinking": false,
                "max_completion_tokens": 300
            }),
        )
    } else {
        (
            format!("{base_url}/responses"),
            json!({
                "model": request.model,
                "instructions": instructions,
                "input": input,
                "stream": true,
                "store": false,
                "max_output_tokens": 300
            }),
        )
    };

    let retry_delays_ms = [200, 600, 1_500, 3_000];
    let mut retry_count = 0;
    let response = loop {
        let retry_client = (retry_count > 0).then(build_http_client);
        let client = retry_client.as_ref().unwrap_or_else(|| http_client());
        let mut request_builder = client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .json(&payload);
        if request.provider != "ollama" {
            request_builder = request_builder.bearer_auth(&api_key);
        } else {
            request_builder = request_builder.timeout(Duration::from_secs(105));
        }
        match request_builder.send().await {
            Ok(response)
                if is_retryable_translation_status(response.status())
                    && retry_count < retry_delays_ms.len() =>
            {
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok());
                let delay = translation_retry_delay(retry_after, retry_count);
                if delay.is_none() {
                    break response;
                }
                retry_count += 1;
                tokio::time::sleep(delay.expect("checked above")).await;
            }
            Ok(response) => break response,
            Err(error)
                if retry_count < retry_delays_ms.len()
                    && (error.is_connect() || error.is_timeout() || error.is_request()) =>
            {
                let retry_delay_ms = retry_delays_ms[retry_count];
                retry_count += 1;
                tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;
            }
            Err(error) => {
                return Err(provider_connection_error(
                    provider_label,
                    "翻译",
                    &error,
                    retry_count,
                    request.provider == "alibaba" && request.workspace_id.trim().is_empty(),
                ))
            }
        }
    };

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let message = compact_error(&request.provider, &body, status);
        emit_translation(&app, &request.segment_id, "error", &message);
        return Err(message);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut received_text = false;
    let mut completed = false;
    let parse_line = if is_ollama {
        ollama_translation_stream_event
    } else {
        translation_stream_event
    };

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|error| format!("翻译响应中断：{error}"))?;
        buffer.extend_from_slice(&bytes);

        while let Some(line) = next_sse_line(&mut buffer) {
            match parse_line(&line) {
                TranslationStreamEvent::Delta(delta) => {
                    received_text = true;
                    emit_translation(&app, &request.segment_id, "delta", &delta);
                }
                TranslationStreamEvent::DeltaAndCompleted(delta) => {
                    received_text = true;
                    completed = true;
                    emit_translation(&app, &request.segment_id, "delta", &delta);
                }
                TranslationStreamEvent::Error(message) => {
                    emit_translation(&app, &request.segment_id, "error", &message);
                    return Err(message);
                }
                TranslationStreamEvent::Completed => completed = true,
                TranslationStreamEvent::Ignore => {}
            }
        }
        if completed {
            break;
        }
    }

    if !completed && !buffer.is_empty() {
        buffer.push(b'\n');
        if let Some(line) = next_sse_line(&mut buffer) {
            match parse_line(&line) {
                TranslationStreamEvent::Delta(delta) => {
                    received_text = true;
                    emit_translation(&app, &request.segment_id, "delta", &delta);
                }
                TranslationStreamEvent::DeltaAndCompleted(delta) => {
                    received_text = true;
                    completed = true;
                    emit_translation(&app, &request.segment_id, "delta", &delta);
                }
                TranslationStreamEvent::Error(message) => {
                    emit_translation(&app, &request.segment_id, "error", &message);
                    return Err(message);
                }
                TranslationStreamEvent::Completed => completed = true,
                TranslationStreamEvent::Ignore => {}
            }
        }
    }

    if let Err(message) = validate_translation_completion(received_text, completed) {
        emit_translation(&app, &request.segment_id, "error", &message);
        return Err(message);
    }

    emit_translation(&app, &request.segment_id, "done", "");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AlibabaAsrState::default())
        .manage(DeepgramAsrState::default())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            apply_pending_restore(&data_dir).map_err(std::io::Error::other)?;
            let database = Database::open(&data_dir).map_err(std::io::Error::other)?;
            app.manage(database);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            has_api_key,
            has_provider_api_key,
            save_api_key,
            save_provider_api_key,
            delete_api_key,
            delete_provider_api_key,
            create_realtime_call,
            start_alibaba_asr,
            send_alibaba_asr_audio,
            finish_alibaba_asr,
            start_deepgram_asr,
            send_deepgram_asr_audio,
            finish_deepgram_asr,
            translate_segment,
            list_courses,
            save_course,
            list_glossary_terms,
            replace_glossary_terms,
            begin_lecture,
            save_lecture_snapshot,
            save_transcript_translation,
            finish_lecture,
            list_lectures,
            delete_lecture,
            get_lecture,
            get_recoverable_lecture,
            add_lecture_bookmark,
            list_lecture_bookmarks,
            delete_lecture_bookmark,
            update_lecture_bookmark,
            import_lecture_slides,
            list_lecture_documents,
            list_document_chunks,
            read_document_source,
            list_lecture_document_keywords,
            delete_lecture_document,
            match_lecture_slide,
            get_chat_history,
            ask_lecture_chat,
            export_lecture_markdown,
            create_app_backup,
            stage_app_restore,
            generate_topic_summary
        ])
        .run(tauri::generate_context!())
        .expect("error while running NUS Lecture Assistant");
}

#[cfg(test)]
mod provider_tests {
    use super::*;

    #[test]
    fn retries_only_transient_translation_statuses() {
        assert!(is_retryable_translation_status(
            reqwest::StatusCode::REQUEST_TIMEOUT
        ));
        assert!(is_retryable_translation_status(
            reqwest::StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(is_retryable_translation_status(
            reqwest::StatusCode::BAD_GATEWAY
        ));
        assert!(is_retryable_translation_status(
            reqwest::StatusCode::SERVICE_UNAVAILABLE
        ));
        assert!(!is_retryable_translation_status(
            reqwest::StatusCode::BAD_REQUEST
        ));
        assert!(!is_retryable_translation_status(
            reqwest::StatusCode::UNAUTHORIZED
        ));
        assert!(!is_retryable_translation_status(
            reqwest::StatusCode::NOT_IMPLEMENTED
        ));
    }

    #[test]
    fn backoff_stays_below_one_second() {
        assert_eq!(
            translation_retry_delay(None, 0),
            Some(Duration::from_millis(200))
        );
        assert_eq!(
            translation_retry_delay(None, 9),
            Some(Duration::from_millis(900))
        );
        assert_eq!(
            translation_retry_delay(Some("2"), 0),
            Some(Duration::from_secs(2))
        );
        assert_eq!(translation_retry_delay(Some("6"), 0), None);
        assert_eq!(
            translation_retry_delay(Some("Wed, 21 Oct 2015 07:28:00 GMT"), 0),
            None
        );
    }

    #[test]
    fn restricts_provider_endpoints() {
        assert_eq!(
            provider_details("alibaba").expect("alibaba").1,
            ALIBABA_BASE_URL
        );
        assert!(provider_details("https://example.com").is_err());
        assert_eq!(
            provider_base_url("alibaba", "llm-classroom").expect("workspace URL"),
            "https://llm-classroom.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1"
        );
        assert!(provider_base_url("alibaba", "https://example.com").is_err());
    }

    #[test]
    fn ollama_cloud_translation_ignores_custom_urls_and_has_an_isolated_key() {
        let request: TranslationRequest = serde_json::from_value(json!({
            "segmentId": "retry-1", "english": "A sentence", "courseName": "Networks",
            "glossary": [], "provider": "ollama-cloud", "workspaceId": "",
            "openaiBaseUrl": "https://example.com/v1",
            "ollamaBaseUrl": "http://127.0.0.1:11434", "model": "cloud-model"
        }))
        .unwrap();
        assert_eq!(
            translation_base_url(&request).unwrap(),
            "https://ollama.com"
        );
        assert_eq!(
            provider_details("ollama-cloud").unwrap().2,
            "ollama-cloud-api-key"
        );
        assert_ne!(
            provider_details("ollama-cloud").unwrap().2,
            provider_details("openai").unwrap().2
        );
        // The global summary/chat resolver does not expose the retry-only cloud provider.
        assert!(text_provider_base_url("ollama-cloud", "", "", "").is_err());
    }

    #[test]
    fn openai_base_url_defaults_to_official_endpoint() {
        assert_eq!(
            normalize_openai_base_url("").expect("default URL"),
            OPENAI_BASE_URL
        );
        assert_eq!(
            provider_base_url_with_openai("openai", "", "").expect("default OpenAI URL"),
            OPENAI_BASE_URL
        );
    }

    #[test]
    fn normalizes_ollama_urls_without_leaking_api_paths() {
        assert_eq!(
            normalize_ollama_base_url(" http://127.0.0.1:11434/api/ ").expect("Ollama URL"),
            "http://127.0.0.1:11434"
        );
        assert_eq!(
            text_provider_base_url("ollama", "", "", "http://localhost:11434/api/chat")
                .expect("Ollama URL"),
            "http://localhost:11434"
        );
        assert!(normalize_ollama_base_url("localhost:11434").is_err());
        assert!(normalize_ollama_base_url("http://user:pass@localhost:11434").is_err());
        assert_eq!(normalize_ollama_base_url("").unwrap(), OLLAMA_BASE_URL);
        assert_eq!(
            normalize_ollama_base_url("https://EXAMPLE.com/local/v1///").unwrap(),
            "https://example.com/local"
        );
        for url in [
            "http:///localhost:11434",
            "ftp://localhost:11434",
            "http://localhost:11434?key=secret",
            "http://localhost:11434#fragment",
        ] {
            assert!(normalize_ollama_base_url(url).is_err(), "{url}");
        }
        assert!(text_provider_base_url("openai", "", "http://localhost:11434", "").is_err());
    }

    #[test]
    fn accepts_and_normalizes_custom_openai_base_url() {
        assert_eq!(
            normalize_openai_base_url(" https://api.example.com/v1/// ").expect("custom URL"),
            "https://api.example.com/v1"
        );
        assert_eq!(
            provider_base_url_with_openai(
                "openai",
                "ignored",
                "https://gateway.example.com/openai/v1/"
            )
            .expect("custom OpenAI URL"),
            "https://gateway.example.com/openai/v1"
        );
    }

    #[test]
    fn rejects_unsafe_openai_base_urls() {
        for value in [
            "http://api.example.com/v1",
            "api.example.com/v1",
            "https:///v1",
            "https://user:pass@api.example.com/v1",
            "https://api.example.com/v1?key=secret",
            "https://api.example.com/v1#fragment",
        ] {
            assert!(
                normalize_openai_base_url(value).is_err(),
                "expected URL to be rejected: {value}"
            );
        }
    }

    #[test]
    fn reads_translation_deltas_from_both_stream_formats() {
        let openai = json!({"type": "response.output_text.delta", "delta": "需求"});
        let alibaba = json!({"choices": [{"delta": {"content": "弹性"}}]});
        assert_eq!(translation_delta(&openai), Some("需求"));
        assert_eq!(translation_delta(&alibaba), Some("弹性"));
    }

    #[test]
    fn preserves_utf8_when_sse_chunks_split_inside_chinese_text() {
        let line = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"需求\"}\n";
        let split = line.find('需').expect("Chinese delta") + 1;
        let mut buffer = line.as_bytes()[..split].to_vec();
        assert_eq!(next_sse_line(&mut buffer), None);
        buffer.extend_from_slice(&line.as_bytes()[split..]);

        let parsed = next_sse_line(&mut buffer).expect("complete SSE line");
        assert_eq!(
            translation_stream_event(&parsed),
            TranslationStreamEvent::Delta("需求".to_string())
        );
    }

    #[test]
    fn surfaces_nested_stream_errors_without_marking_them_complete() {
        assert_eq!(
            translation_stream_event(
                "data: {\"type\":\"error\",\"error\":{\"message\":\"quota exceeded\"}}"
            ),
            TranslationStreamEvent::Error("quota exceeded".to_string())
        );
    }

    #[test]
    fn parses_ollama_ndjson_unicode_delta_and_done() {
        assert_eq!(
            ollama_translation_stream_event(r#"{"message":{"content":"需求"},"done":false}"#),
            TranslationStreamEvent::Delta("需求".to_string())
        );
        assert_eq!(
            ollama_translation_stream_event(r#"{"done":true,"done_reason":"stop"}"#),
            TranslationStreamEvent::Completed
        );
        assert_eq!(
            ollama_translation_stream_event(r#"{"error":"model not found"}"#),
            TranslationStreamEvent::Error("Ollama 返回错误：model not found".to_string())
        );
        assert!(validate_translation_completion(true, false).is_err());
    }

    #[test]
    fn rejects_ollama_length_truncation() {
        assert_eq!(
            ollama_translation_stream_event(
                r#"{"message":{"content":"未完"},"done":true,"done_reason":"length"}"#
            ),
            TranslationStreamEvent::Error(
                "Ollama 输出达到长度上限，内容可能被截断；请缩短输入或更换模型".to_string()
            )
        );
    }

    #[test]
    fn preserves_ollama_unicode_across_every_ndjson_chunk_boundary() {
        let line = "{\"message\":{\"content\":\"需求与弹性\"},\"done\":false}\r\n";
        for split in 0..line.len() {
            let mut buffer = line.as_bytes()[..split].to_vec();
            assert_eq!(next_sse_line(&mut buffer), None);
            buffer.extend_from_slice(&line.as_bytes()[split..]);
            let parsed = next_sse_line(&mut buffer).unwrap();
            assert_eq!(
                ollama_translation_stream_event(&parsed),
                TranslationStreamEvent::Delta("需求与弹性".to_string())
            );
            assert!(buffer.is_empty());
        }
    }

    #[test]
    fn handles_ollama_final_text_and_rejects_malformed_or_unfinished_output() {
        assert_eq!(
            ollama_translation_stream_event(
                r#"{"message":{"content":"完成"},"done":true,"done_reason":"stop"}"#
            ),
            TranslationStreamEvent::DeltaAndCompleted("完成".to_string())
        );
        assert!(matches!(
            ollama_translation_stream_event(r#"{"message":{"content":"未结束"}"#),
            TranslationStreamEvent::Error(_)
        ));
        assert!(validate_ollama_response(&json!({"message": {"content": "部分"}})).is_err());
        assert!(validate_translation_completion(false, true).is_err());
        assert!(validate_translation_completion(true, true).is_ok());
    }

    #[test]
    fn recognizes_cloud_completion_and_explicit_truncation() {
        assert_eq!(
            translation_stream_event("data: [DONE]"),
            TranslationStreamEvent::Completed
        );
        assert_eq!(
            translation_stream_event(r#"data: {"choices":[{"finish_reason":"stop"}]}"#),
            TranslationStreamEvent::Completed
        );
        assert!(matches!(
            translation_stream_event(r#"data: {"choices":[{"finish_reason":"length"}]}"#),
            TranslationStreamEvent::Error(_)
        ));
        assert!(matches!(
            translation_stream_event(r#"data: {"type":"response.incomplete"}"#),
            TranslationStreamEvent::Error(_)
        ));
    }

    #[test]
    fn bounds_local_context_without_silently_dropping_material() {
        assert!(validate_ollama_input(&[&"a".repeat(OLLAMA_INPUT_BYTES)]).is_ok());
        assert!(validate_ollama_input(&[&"a".repeat(OLLAMA_INPUT_BYTES), "中"]).is_err());
        assert!(validate_ollama_input(&[&"中".repeat(OLLAMA_INPUT_BYTES / 3 + 1)]).is_err());
        assert!(OLLAMA_INPUT_BYTES + OLLAMA_OUTPUT_TOKENS + 512 < OLLAMA_CONTEXT_TOKENS);
    }

    #[test]
    fn converts_course_glossary_for_qwen_mt() {
        let terms = qwen_mt_terms(&[
            "elasticity → 弹性；别名：responsiveness".to_string(),
            "NUS → 保留英文".to_string(),
        ]);

        assert_eq!(terms[0], json!({"source": "elasticity", "target": "弹性"}));
        assert_eq!(terms[1], json!({"source": "NUS", "target": "NUS"}));
    }

    #[test]
    fn prioritizes_sentence_terms_without_sending_the_entire_glossary() {
        let mut glossary = (0..40)
            .map(|index| format!("term {index} → 术语 {index}"))
            .collect::<Vec<_>>();
        glossary.push("Singtel → 新加坡电信；别名：Singapore Telecom".to_string());

        let selected = relevant_glossary(
            &glossary,
            "Singtel and StarHub operate cellular networks in Singapore.",
            12,
        );

        assert!(selected.iter().any(|entry| entry.starts_with("Singtel →")));
        assert_eq!(selected.len(), 9);
    }

    #[test]
    fn adds_course_vocabulary_to_realtime_prompt() {
        let prompt = transcription_prompt(&RealtimeConfig {
            course_name: "Cellular Networks".to_string(),
            course_context: "CEG5104".to_string(),
            keywords: vec!["Singtel".to_string(), "StarHub".to_string()],
            transcription_model: "gpt-live-transcribe".to_string(),
        });
        assert!(prompt.contains("Singtel, StarHub"));
        assert!(prompt.contains("Spell this course vocabulary exactly"));
    }
}
