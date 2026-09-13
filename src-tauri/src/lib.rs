#![recursion_limit = "256"]

use futures_util::StreamExt;
use keyring::Entry;
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    error::Error as StdError,
    net::IpAddr,
    sync::OnceLock,
    time::{Duration, Instant},
};
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
    save_lecture_snapshot, update_lecture_bookmark, Database,
};
use summary::generate_topic_summary;

const KEYRING_SERVICE: &str = "com.nus.lecture-assistant";
const OPENAI_KEYRING_USER: &str = "openai-api-key";
const ALIBABA_KEYRING_USER: &str = "alibaba-api-key";
const ALIBABA_ASR_KEYRING_USER: &str = "alibaba-asr-api-key";
const DEEPGRAM_KEYRING_USER: &str = "deepgram-api-key";
const DEEPGRAM_LISTEN_URL: &str = "https://api.deepgram.com/v1/listen";
const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const ALIBABA_BASE_URL: &str = "https://dashscope-intl.aliyuncs.com/compatible-mode/v1";
static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static TRANSLATION_GATE: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
static LOCAL_TRANSLATION_GATE: OnceLock<tokio::sync::Semaphore> = OnceLock::new();

pub(crate) fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(45))
        .tcp_keepalive(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn build_local_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .map_err(|error| format!("无法创建本地翻译 HTTP 客户端：{error}"))
}

fn build_deepgram_test_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(12))
        .tcp_keepalive(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
        .build()
        .map_err(|error| format!("无法创建 Deepgram 测试客户端：{error}"))
}

pub(crate) fn http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(build_http_client)
}

fn translation_gate() -> &'static tokio::sync::Semaphore {
    TRANSLATION_GATE.get_or_init(|| tokio::sync::Semaphore::new(2))
}

fn local_translation_gate() -> &'static tokio::sync::Semaphore {
    LOCAL_TRANSLATION_GATE.get_or_init(|| tokio::sync::Semaphore::new(1))
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
        "；当前使用阿里云共享线路，请在设置中填写 ws- 或 llm- 开头的业务空间 ID"
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
    model: String,
    local_endpoint: Option<String>,
    #[serde(default)]
    openai_base_url: Option<String>,
    #[serde(default)]
    key_slot: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalTranslationTestResult {
    text: String,
    elapsed_ms: u128,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeepgramConnectionTestResult {
    model: String,
    elapsed_ms: u128,
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
        "alibaba" => Ok(("阿里云百炼", ALIBABA_BASE_URL, ALIBABA_KEYRING_USER)),
        "alibaba-asr" => Ok(("阿里云实时转写", "", ALIBABA_ASR_KEYRING_USER)),
        "deepgram" => Ok((
            "Deepgram",
            "https://api.deepgram.com/v1",
            DEEPGRAM_KEYRING_USER,
        )),
        "local" => Ok(("本地翻译模型", "", "")),
        _ => Err("不支持的模型供应商".to_string()),
    }
}

fn alibaba_workspace_host(workspace_id: &str) -> Result<String, String> {
    let workspace_id = workspace_id.trim();
    if workspace_id.is_empty() {
        return Ok("dashscope-intl.aliyuncs.com".to_string());
    }
    let valid_prefix = workspace_id.starts_with("llm-") || workspace_id.starts_with("ws-");
    let valid = valid_prefix
        && workspace_id.len() <= 80
        && workspace_id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        });
    if !valid {
        return Err("阿里云业务空间 ID 格式不正确，应类似 ws-xxxxxxxx 或 llm-xxxxxxxx".to_string());
    }
    Ok(format!("{workspace_id}.ap-southeast-1.maas.aliyuncs.com"))
}

pub(crate) fn provider_base_url(provider: &str, workspace_id: &str) -> Result<String, String> {
    match provider {
        "openai" => Ok(OPENAI_BASE_URL.to_string()),
        "alibaba" => Ok(format!(
            "https://{}/compatible-mode/v1",
            alibaba_workspace_host(workspace_id)?
        )),
        "local" => Err("本地翻译服务地址需要单独配置".to_string()),
        _ => Err("不支持的模型供应商".to_string()),
    }
}

pub(crate) fn custom_openai_base_url(value: Option<&str>) -> Result<String, String> {
    let value = value.unwrap_or(OPENAI_BASE_URL).trim().trim_end_matches('/');
    let url = reqwest::Url::parse(value).map_err(|_| "OpenAI 兼容地址格式无效".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() || url.path().is_empty() {
        return Err("OpenAI 兼容地址必须是无凭据、无参数的 HTTP/HTTPS 地址".to_string());
    }
    Ok(value.to_string())
}

fn local_translation_base_url(endpoint: &str) -> Result<String, String> {
    if endpoint.len() > 300 {
        return Err("本地翻译服务地址过长".to_string());
    }
    let mut url =
        reqwest::Url::parse(endpoint.trim()).map_err(|_| "本地翻译服务地址格式无效".to_string())?;
    if url.scheme() != "http" || !url.username().is_empty() || url.password().is_some() {
        return Err("本地翻译服务必须使用无凭据的 http 回环地址".to_string());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "本地翻译服务地址缺少主机".to_string())?
        .trim_matches(['[', ']']);
    let is_loopback = host == "localhost"
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false);
    if !is_loopback {
        return Err("本地翻译服务仅允许本机回环地址".to_string());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("本地翻译服务地址不能包含查询参数或片段".to_string());
    }
    match url.path() {
        "" | "/" => url.set_path("/v1"),
        "/v1" | "/v1/" => url.set_path("/v1"),
        _ => return Err("本地翻译服务地址路径必须为 /v1".to_string()),
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

fn keyring_entry(provider: &str) -> Result<Entry, String> {
    let (_, _, user) = provider_details(provider)?;
    Entry::new(KEYRING_SERVICE, user).map_err(|error| format!("无法访问系统凭据库：{error}"))
}

fn read_api_key() -> Result<String, String> {
    read_provider_api_key("openai")
}

pub(crate) fn read_provider_api_key(provider: &str) -> Result<String, String> {
    let (label, _, _) = provider_details(provider)?;
    keyring_entry(provider)?
        .get_password()
        .map_err(|_| format!("尚未保存 {label} API Key"))
}

pub(crate) fn read_provider_api_key_slot(provider: &str, slot: Option<&str>) -> Result<String, String> {
    let Some(slot) = slot.filter(|value| !value.trim().is_empty()) else { return read_provider_api_key(provider); };
    if provider != "openai" { return read_provider_api_key(provider); }
    let slot_user = format!("openai-{slot}-api-key");
    Entry::new(KEYRING_SERVICE, &slot_user).map_err(|error| format!("无法访问系统凭据库：{error}"))?.get_password().or_else(|_| read_provider_api_key(provider)).map_err(|_| format!("尚未保存 OpenAI {slot} API Key"))
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
    if !valid_provider_api_key(&provider, trimmed) {
        return Err(format!("{label} API Key 格式看起来不正确"));
    }

    keyring_entry(&provider)?
        .set_password(trimmed)
        .map_err(|error| format!("无法保存到系统凭据库：{error}"))
}

fn valid_provider_api_key(provider: &str, api_key: &str) -> bool {
    let safe_characters = api_key.bytes().all(|byte| byte.is_ascii_graphic());
    if !safe_characters || !(20..=512).contains(&api_key.len()) {
        return false;
    }
    match provider {
        "deepgram" => api_key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
        "openai" | "alibaba" | "alibaba-asr" => api_key.starts_with("sk-"),
        _ => false,
    }
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

fn chat_completion_text(event: &Value) -> Option<&str> {
    event
        .get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?
        .as_str()
}

fn deepgram_test_url(model: &str) -> Result<reqwest::Url, String> {
    let model = model.trim();
    if !matches!(model, "nova-3" | "nova-2") {
        return Err("当前不支持所选 Deepgram 实时转写模型".to_string());
    }
    let mut url = reqwest::Url::parse(DEEPGRAM_LISTEN_URL)
        .map_err(|_| "Deepgram 测试地址无效".to_string())?;
    url.query_pairs_mut()
        .append_pair("model", model)
        .append_pair("language", "en")
        .append_pair("smart_format", "true");
    Ok(url)
}

fn deepgram_test_wav() -> Vec<u8> {
    const SAMPLE_RATE: u32 = 16_000;
    const CHANNELS: u16 = 1;
    const BITS_PER_SAMPLE: u16 = 16;
    const DURATION_MS: u32 = 500;
    let sample_count = SAMPLE_RATE * DURATION_MS / 1_000;
    let block_align = CHANNELS * (BITS_PER_SAMPLE / 8);
    let byte_rate = SAMPLE_RATE * u32::from(block_align);
    let data_length = sample_count * u32::from(block_align);
    let mut wav = Vec::with_capacity(44 + data_length as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_length).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_length.to_le_bytes());
    wav.resize(44 + data_length as usize, 0);
    wav
}

fn deepgram_test_http_error(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        400 => {
            "所选 Deepgram 模型不可用或测试请求被拒绝（HTTP 400），请检查 Nova 模型权限".to_string()
        }
        401 | 403 => "Deepgram 拒绝授权，请检查已保存的 API Key、权限和账户状态".to_string(),
        402 => "Deepgram 账户额度不足，请充值后重试".to_string(),
        429 => "Deepgram 请求受限，请稍后重试或检查账户并发限制".to_string(),
        code => format!("Deepgram 连接测试失败（HTTP {code}）"),
    }
}

#[tauri::command]
async fn test_deepgram_connection(model: String) -> Result<DeepgramConnectionTestResult, String> {
    let url = deepgram_test_url(&model)?;
    let api_key = tauri::async_runtime::spawn_blocking(|| read_provider_api_key("deepgram"))
        .await
        .map_err(|_| "无法读取 Deepgram API Key".to_string())??;
    if !valid_provider_api_key("deepgram", &api_key) {
        return Err("已保存的 Deepgram API Key 格式无效".to_string());
    }
    let mut authorization = reqwest::header::HeaderValue::from_str(&format!("Token {api_key}"))
        .map_err(|_| "已保存的 Deepgram API Key 无法用于授权".to_string())?;
    authorization.set_sensitive(true);
    let started = Instant::now();
    let response = build_deepgram_test_http_client()?
        .post(url)
        .header(reqwest::header::AUTHORIZATION, authorization)
        .header(reqwest::header::CONTENT_TYPE, "audio/wav")
        .body(deepgram_test_wav())
        .send()
        .await
        .map_err(|error| provider_connection_error("Deepgram", "连接测试", &error, 0, false))?;
    if !response.status().is_success() {
        return Err(deepgram_test_http_error(response.status()));
    }
    Ok(DeepgramConnectionTestResult {
        model: model.trim().to_string(),
        elapsed_ms: started.elapsed().as_millis(),
    })
}

#[tauri::command]
async fn test_local_translation(
    endpoint: String,
    model: String,
) -> Result<LocalTranslationTestResult, String> {
    let model = model.trim();
    if model.is_empty() || model.chars().count() > 100 {
        return Err("本地翻译模型名称无效".to_string());
    }
    let base_url = local_translation_base_url(&endpoint)?;
    let started = Instant::now();
    let response = build_local_http_client()?
        .post(format!("{base_url}/chat/completions"))
        .json(&json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": "Translate English into concise Simplified Chinese. Return only the translation."
                },
                {"role": "user", "content": "The gradient does not converge when the learning rate exceeds 0.1."}
            ],
            "stream": false,
            "temperature": 0,
            "max_tokens": 80,
            "reasoning_effort": "none"
        }))
        .send()
        .await
        .map_err(|error| format!("无法连接本地翻译服务：{error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("无法读取本地翻译响应：{error}"))?;
    if !status.is_success() {
        return Err(compact_error("local", &body, status));
    }
    let value = serde_json::from_str::<Value>(&body)
        .map_err(|_| "本地翻译服务返回了无效 JSON".to_string())?;
    let text = chat_completion_text(&value)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "本地翻译服务没有返回译文".to_string())?;
    Ok(LocalTranslationTestResult {
        text: clip_characters(text, 160),
        elapsed_ms: started.elapsed().as_millis(),
    })
}

#[derive(Debug, PartialEq, Eq)]
enum TranslationStreamEvent {
    Delta(String),
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
    let Some(data) = line.strip_prefix("data:").map(str::trim_start) else {
        return TranslationStreamEvent::Ignore;
    };
    if data.is_empty() || data == "[DONE]" {
        return TranslationStreamEvent::Ignore;
    }
    let Ok(event) = serde_json::from_str::<Value>(data) else {
        return TranslationStreamEvent::Ignore;
    };
    if let Some(delta) = translation_delta(&event).filter(|delta| !delta.is_empty()) {
        return TranslationStreamEvent::Delta(clean_translation_markdown(delta));
    }
    match event.get("type").and_then(Value::as_str) {
        Some("response.completed") => TranslationStreamEvent::Completed,
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

fn clean_translation_markdown(text: &str) -> String {
    text.replace("**", "")
}

#[cfg(test)]
mod markdown_cleaning_tests {
    use super::*;

    #[test]
    fn removes_markdown_bold_markers_from_translation() {
        assert_eq!(clean_translation_markdown("普通文本"), "普通文本");
        assert_eq!(clean_translation_markdown("**主要负责注册和跟踪功能**"), "主要负责注册和跟踪功能");
        assert_eq!(clean_translation_markdown("从而**主要负责**注册"), "从而主要负责注册");
        assert_eq!(clean_translation_markdown("**核心**概念和**重点**"), "核心概念和重点");
        assert_eq!(clean_translation_markdown("**"), "");
    }
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
    let english = request.english.trim();
    let model = request.model.trim();
    if english.is_empty() {
        return Err("没有可翻译的英文内容".to_string());
    }
    if english.chars().count() > 8_000 || request.segment_id.chars().count() > 200 {
        return Err("待翻译内容异常长".to_string());
    }
    if model.is_empty() || model.chars().count() > 100 {
        return Err("翻译模型名称无效".to_string());
    }
    let (provider_label, _, _) = provider_details(&request.provider)?;
    let is_local = request.provider == "local";
    let base_url = if is_local {
        local_translation_base_url(request.local_endpoint.as_deref().unwrap_or_default())?
    } else {
        if request.provider == "openai" { custom_openai_base_url(request.openai_base_url.as_deref())? } else { provider_base_url(&request.provider, &request.workspace_id)? }
    };
    let _permit = if is_local {
        local_translation_gate().acquire().await
    } else {
        translation_gate().acquire().await
    }
    .map_err(|_| "翻译队列已关闭".to_string())?;
    let api_key = if is_local {
        None
    } else {
        Some(read_provider_api_key_slot(&request.provider, request.key_slot.as_deref())?)
    };
    let previous = clip_characters(
        request.previous_english.as_deref().unwrap_or_default(),
        2_000,
    );
    let selected_glossary = relevant_glossary(
        &request.glossary,
        &format!("{previous}\n{english}"),
        if model.starts_with("qwen-mt-") {
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

    let instructions = "Translate the lecturer's English into concise, natural Simplified Chinese. Preserve all technical terms, numbers, equations, negation, uncertainty, and emphasis. Follow the supplied glossary exactly; when an entry says 保留英文, copy that source token unchanged and do not translate or expand it. Return only the Chinese translation as plain text, with no markdown formatting, labels, explanation, or visible reasoning.";
    let is_qwen_mt = request.provider == "alibaba" && model.starts_with("qwen-mt-");
    let (endpoint, payload) = if is_local {
        (
            format!("{base_url}/chat/completions"),
            json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": instructions},
                    {"role": "user", "content": input}
                ],
                "stream": true,
                "temperature": 0,
                "max_tokens": 180,
                "reasoning_effort": "none"
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
                "model": model,
                "messages": [{"role": "user", "content": english}],
                "stream": true,
                "translation_options": translation_options
            }),
        )
    } else if request.provider == "alibaba" {
        (
            format!("{base_url}/chat/completions"),
            json!({
                "model": model,
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
                "model": model,
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
    let local_client = if is_local {
        Some(build_local_http_client()?)
    } else {
        None
    };
    let response = loop {
        let retry_client = (!is_local && retry_count > 0).then(build_http_client);
        let client = if is_local {
            local_client
                .as_ref()
                .expect("本地翻译客户端应在请求循环前创建")
        } else {
            retry_client.as_ref().unwrap_or_else(|| http_client())
        };
        let mut request_builder = client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .json(&payload);
        if let Some(api_key) = api_key.as_deref() {
            request_builder = request_builder.bearer_auth(api_key);
        }
        match request_builder.send().await {
            Ok(response) => break response,
            Err(error)
                if retry_count < if is_local { 0 } else { retry_delays_ms.len() }
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
    let mut raw_response = Vec::new();
    let mut received_text = false;

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|error| format!("翻译响应中断：{error}"))?;
        raw_response.extend_from_slice(&bytes);
        buffer.extend_from_slice(&bytes);

        while let Some(line) = next_sse_line(&mut buffer) {
            match translation_stream_event(&line) {
                TranslationStreamEvent::Delta(delta) => {
                    received_text = true;
                    emit_translation(&app, &request.segment_id, "delta", &delta);
                }
                TranslationStreamEvent::Error(message) => {
                    emit_translation(&app, &request.segment_id, "error", &message);
                    return Err(message);
                }
                TranslationStreamEvent::Completed | TranslationStreamEvent::Ignore => {}
            }
        }
    }

    if !buffer.is_empty() {
        buffer.push(b'\n');
        if let Some(line) = next_sse_line(&mut buffer) {
            match translation_stream_event(&line) {
                TranslationStreamEvent::Delta(delta) => {
                    received_text = true;
                    emit_translation(&app, &request.segment_id, "delta", &delta);
                }
                TranslationStreamEvent::Error(message) => {
                    emit_translation(&app, &request.segment_id, "error", &message);
                    return Err(message);
                }
                TranslationStreamEvent::Completed | TranslationStreamEvent::Ignore => {}
            }
        }
    }

    if !received_text {
        if let Ok(value) = serde_json::from_slice::<Value>(&raw_response) {
            if let Some(text) = chat_completion_text(&value)
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                received_text = true;
                emit_translation(&app, &request.segment_id, "delta", text);
            }
        }
    }

    if !received_text {
        let message = "翻译服务没有返回文本";
        emit_translation(&app, &request.segment_id, "error", message);
        return Err(message.to_string());
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
            test_deepgram_connection,
            test_local_translation,
            translate_segment,
            list_courses,
            save_course,
            list_glossary_terms,
            replace_glossary_terms,
            begin_lecture,
            save_lecture_snapshot,
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
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    fn read_http_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|error| format!("set timeout: {error}"))?;
        let mut request = Vec::new();
        let mut chunk = [0_u8; 4096];
        let header_end;
        let content_length;
        loop {
            let count = stream
                .read(&mut chunk)
                .map_err(|error| format!("read request: {error}"))?;
            if count == 0 {
                return Err("request ended before headers were complete".to_string());
            }
            request.extend_from_slice(&chunk[..count]);
            let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            header_end = end + 4;
            content_length = String::from_utf8_lossy(&request[..end])
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            break;
        }
        while request.len() < header_end + content_length {
            let count = stream
                .read(&mut chunk)
                .map_err(|error| format!("read request body: {error}"))?;
            if count == 0 {
                return Err("request ended before body was complete".to_string());
            }
            request.extend_from_slice(&chunk[..count]);
        }
        Ok(request)
    }

    fn serve_local_translation_request(listener: TcpListener) -> Result<(), String> {
        let (mut stream, _) = listener
            .accept()
            .map_err(|error| format!("accept request: {error}"))?;
        let request = read_http_request(&mut stream)?;
        let end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or_else(|| "request has no header terminator".to_string())?;
        let header_text = String::from_utf8_lossy(&request[..end]);
        let body_bytes = &request[end + 4..];
        let request_line = header_text.lines().next().unwrap_or_default();
        if request_line != "POST /v1/chat/completions HTTP/1.1" {
            return Err(format!("unexpected request line: {request_line}"));
        }
        let payload: Value = serde_json::from_slice(body_bytes)
            .map_err(|error| format!("invalid request JSON: {error}"))?;
        if payload.get("model").and_then(Value::as_str) != Some("test-model") {
            return Err("trimmed local model was not forwarded".to_string());
        }
        if payload.get("stream").and_then(Value::as_bool) != Some(false) {
            return Err("local test request must be non-streaming".to_string());
        }
        if payload.get("reasoning_effort").and_then(Value::as_str) != Some("none") {
            return Err("local test request must disable reasoning".to_string());
        }
        let response_body = r#"{"id":"test","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"梯度不会收敛。"},"finish_reason":"stop"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .map_err(|error| format!("write response: {error}"))
    }

    #[test]
    fn validates_keys_per_provider_without_requiring_deepgram_sk_prefix() {
        assert!(valid_provider_api_key("deepgram", &"a".repeat(40)));
        assert!(!valid_provider_api_key("openai", &"a".repeat(40)));
        assert!(valid_provider_api_key(
            "openai",
            "sk-example-key-for-testing"
        ));
        assert!(!valid_provider_api_key("deepgram", "short"));
        assert!(!valid_provider_api_key(
            "deepgram",
            "example-key-with spaces-invalid"
        ));
        assert!(!valid_provider_api_key(
            "deepgram",
            "example-key-with\r\nheader-injection"
        ));
        assert!(!valid_provider_api_key("unknown", &"a".repeat(40)));
        assert_eq!(provider_details("deepgram").unwrap().2, "deepgram-api-key");
        assert!(provider_base_url("deepgram", "").is_err());
    }

    #[test]
    fn keeps_deepgram_connection_tests_on_the_official_host_and_supported_models() {
        for model in ["nova-3", "nova-2"] {
            let url = deepgram_test_url(model).expect("supported Deepgram model");
            assert_eq!(url.scheme(), "https");
            assert_eq!(url.host_str(), Some("api.deepgram.com"));
            assert_eq!(url.path(), "/v1/listen");
            let pairs = url.query_pairs().collect::<Vec<_>>();
            assert!(pairs
                .iter()
                .any(|pair| pair.0 == "model" && pair.1 == model));
            assert!(pairs
                .iter()
                .any(|pair| pair.0 == "language" && pair.1 == "en"));
        }
        assert!(deepgram_test_url("nova-3&callback=https://example.com").is_err());
        assert!(deepgram_test_url("whisper").is_err());
    }

    #[test]
    fn builds_a_small_valid_pcm_wav_for_deepgram_connection_tests() {
        let wav = deepgram_test_wav();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 16_000);
        assert_eq!(wav.len(), 16_044);
        assert!(wav[44..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn classifies_deepgram_connection_test_failures_without_response_details() {
        assert!(deepgram_test_http_error(reqwest::StatusCode::BAD_REQUEST).contains("模型不可用"));
        assert!(deepgram_test_http_error(reqwest::StatusCode::UNAUTHORIZED).contains("拒绝授权"));
        assert!(deepgram_test_http_error(reqwest::StatusCode::FORBIDDEN).contains("拒绝授权"));
        assert!(
            deepgram_test_http_error(reqwest::StatusCode::PAYMENT_REQUIRED).contains("额度不足")
        );
        assert!(
            deepgram_test_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS).contains("请求受限")
        );
        assert_eq!(
            deepgram_test_http_error(reqwest::StatusCode::SERVICE_UNAVAILABLE),
            "Deepgram 连接测试失败（HTTP 503）"
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
        assert_eq!(
            provider_base_url("alibaba", "ws-classroom").expect("current workspace URL"),
            "https://ws-classroom.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1"
        );
        assert!(provider_base_url("alibaba", "https://example.com").is_err());
    }

    #[test]
    fn accepts_only_loopback_local_translation_endpoints() {
        assert_eq!(
            local_translation_base_url("http://127.0.0.1:11434/v1/").unwrap(),
            "http://127.0.0.1:11434/v1"
        );
        assert_eq!(
            local_translation_base_url("http://localhost/v1").unwrap(),
            "http://localhost/v1"
        );
        assert!(local_translation_base_url("https://127.0.0.1:11434/v1").is_err());
        assert!(local_translation_base_url("http://192.168.1.10:11434/v1").is_err());
        assert!(local_translation_base_url("http://127.0.0.1:11434/api").is_err());
        assert!(local_translation_base_url("http://127.0.0.1:11434/v1?token=x").is_err());
    }

    #[test]
    #[ignore = "requires permission to bind a loopback socket"]
    fn tests_local_translation_against_a_loopback_chat_server() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback test server");
        let port = listener.local_addr().expect("test server address").port();
        let server = std::thread::spawn(move || serve_local_translation_request(listener));

        let result = tauri::async_runtime::block_on(test_local_translation(
            format!("http://127.0.0.1:{port}/v1"),
            "  test-model  ".to_string(),
        ))
        .expect("local translation test should succeed");

        assert_eq!(result.text, "梯度不会收敛。");
        assert!(result.elapsed_ms < 2_000);
        server
            .join()
            .expect("test server thread should not panic")
            .expect("test server should receive the expected request");
    }

    #[test]
    fn reads_translation_deltas_from_both_stream_formats() {
        let openai = json!({"type": "response.output_text.delta", "delta": "需求"});
        let alibaba = json!({"choices": [{"delta": {"content": "弹性"}}]});
        assert_eq!(translation_delta(&openai), Some("需求"));
        assert_eq!(translation_delta(&alibaba), Some("弹性"));
    }

    #[test]
    fn reads_non_streaming_chat_completion_text() {
        let response =
            json!({"choices": [{"message": {"content": "学习率过高时，梯度不会收敛。"}}]});
        assert_eq!(
            chat_completion_text(&response),
            Some("学习率过高时，梯度不会收敛。")
        );
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
