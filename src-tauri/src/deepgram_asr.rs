use base64::prelude::*;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, Message},
};
use uuid::Uuid;

const MODEL: &str = "nova-3";
const ENDPOINT: &str = "wss://api.deepgram.com/v1/listen";

pub struct DeepgramAsrState {
    session: Mutex<Option<DeepgramSession>>,
}

impl Default for DeepgramAsrState {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
        }
    }
}

struct DeepgramSession {
    id: String,
    command_tx: mpsc::Sender<Command>,
}

enum Command {
    Audio(Vec<u8>),
    Finish(oneshot::Sender<Result<(), String>>),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDeepgramAsrRequest {
    pub model: String,
    pub elapsed_offset_ms: i64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DeepgramAsrEvent {
    kind: String,
    item_id: String,
    text: String,
    start_ms: i64,
    end_ms: Option<i64>,
}

fn emit(app: &AppHandle, event: DeepgramAsrEvent) {
    let _ = app.emit("deepgram-asr-event", event);
}

fn emit_error(app: &AppHandle, id: &str, message: impl Into<String>) {
    emit(
        app,
        DeepgramAsrEvent {
            kind: "error".to_string(),
            item_id: id.to_string(),
            text: message.into(),
            start_ms: 0,
            end_ms: None,
        },
    );
}

#[derive(Debug, Deserialize)]
struct ResultMessage {
    #[serde(rename = "type")]
    message_type: Option<String>,
    channel: Option<Channel>,
    #[serde(rename = "is_final")]
    is_final: Option<bool>,
    #[serde(rename = "speech_final")]
    speech_final: Option<bool>,
    start: Option<f64>,
    duration: Option<f64>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Channel {
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
struct Alternative {
    transcript: String,
}

struct ParsedResult {
    event: DeepgramAsrEvent,
    is_final: bool,
    speech_final: bool,
}

fn parse_result(value: &str, offset_ms: i64) -> Option<ParsedResult> {
    let result: ResultMessage = serde_json::from_str(value).ok()?;
    if result.message_type.as_deref() != Some("Results") {
        return None;
    }
    let text = result
        .channel?
        .alternatives
        .first()?
        .transcript
        .trim()
        .to_string();
    if text.is_empty() {
        return None;
    }
    let start_ms = offset_ms + (result.start.unwrap_or_default() * 1000.0).round() as i64;
    let end_ms = result.duration.map(|duration| {
        offset_ms + ((result.start.unwrap_or_default() + duration) * 1000.0).round() as i64
    });
    let item_id = format!("deepgram:{start_ms}");
    Some(ParsedResult {
        event: DeepgramAsrEvent {
            kind: "delta".to_string(),
            item_id,
            text,
            start_ms,
            end_ms,
        },
        is_final: result.is_final.unwrap_or(false),
        speech_final: result.speech_final.unwrap_or(false),
    })
}

#[tauri::command]
pub async fn start_deepgram_asr(
    app: AppHandle,
    state: State<'_, DeepgramAsrState>,
    request: StartDeepgramAsrRequest,
) -> Result<(), String> {
    if !request.model.is_empty() && request.model != MODEL {
        return Err(format!("Deepgram 暂不支持模型 {}", request.model));
    }
    if state.session.lock().await.is_some() {
        return Err("Deepgram 实时转写会话已经启动".to_string());
    }
    let api_key = super::read_provider_api_key("deepgram")?;
    let endpoint = format!(
        "{ENDPOINT}?model={MODEL}&encoding=linear16&sample_rate=16000&channels=1&language=en&interim_results=true&punctuate=true&smart_format=true&endpointing=800&utterance_end_ms=1200"
    );
    let mut websocket_request = endpoint
        .into_client_request()
        .map_err(|error| format!("无法创建 Deepgram 实时转写请求：{error}"))?;
    websocket_request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Token {api_key}"))
            .map_err(|error| format!("Deepgram 授权信息无效：{error}"))?,
    );
    websocket_request.headers_mut().insert(
        "User-Agent",
        HeaderValue::from_static("nus-lecture-assistant/0.13.0"),
    );
    let (websocket, _) = connect_async(websocket_request)
        .await
        .map_err(|error| format!("无法连接 Deepgram 实时转写：{error}"))?;
    let (mut writer, mut reader) = websocket.split();
    let id = Uuid::new_v4().to_string();
    let (command_tx, mut command_rx) = mpsc::channel::<Command>(24);
    *state.session.lock().await = Some(DeepgramSession {
        id: id.clone(),
        command_tx: command_tx.clone(),
    });
    let offset_ms = request.elapsed_offset_ms;
    tauri::async_runtime::spawn(async move {
        let mut finish_waiter: Option<oneshot::Sender<Result<(), String>>> = None;
        // Deepgram can mark a stable chunk with `is_final` before the speaker
        // has finished the utterance. Accumulate those chunks and only emit a
        // final event on `speech_final`, otherwise translation starts mid-sentence.
        let mut pending_item_id: Option<String> = None;
        let mut pending_text = String::new();
        loop {
            tokio::select! {
                command = command_rx.recv() => match command {
                    Some(Command::Audio(audio)) => {
                        if let Err(error) = writer.send(Message::Binary(audio.into())).await {
                            emit_error(&app, &id, format!("发送课堂音频失败：{error}"));
                            break;
                        }
                    }
                    Some(Command::Finish(waiter)) => {
                        finish_waiter = Some(waiter);
                        if let Err(error) = writer.send(Message::Text(json!({"type":"CloseStream"}).to_string().into())).await {
                            emit_error(&app, &id, format!("结束 Deepgram 转写失败：{error}"));
                            break;
                        }
                    }
                    None => break,
                },
                message = reader.next() => {
                    let Some(message) = message else { break; };
                    match message {
                        Ok(Message::Text(text)) => {
                            if let Ok(value) = serde_json::from_str::<ResultMessage>(text.as_str()) {
                                if value.message_type.as_deref() == Some("Error") {
                                    let message = value.error.unwrap_or_else(|| "Deepgram 返回未知错误".to_string());
                                    emit_error(&app, &id, message.clone());
                                    if let Some(waiter) = finish_waiter.take() { let _ = waiter.send(Err(message)); }
                                    break;
                                }
                            }
                            if let Some(parsed) = parse_result(text.as_str(), offset_ms) {
                                let item_id = pending_item_id
                                    .get_or_insert_with(|| parsed.event.item_id.clone())
                                    .clone();
                                let combined_text = if parsed.is_final {
                                    if !pending_text.is_empty() {
                                        pending_text.push(' ');
                                    }
                                    pending_text.push_str(&parsed.event.text);
                                    pending_text.clone()
                                } else if pending_text.is_empty() {
                                    parsed.event.text.clone()
                                } else {
                                    format!("{} {}", pending_text, parsed.event.text)
                                };
                                let speech_final = parsed.speech_final;
                                let finishing = finish_waiter.is_some();
                                let final_result = speech_final || (finishing && parsed.is_final);
                                emit(&app, DeepgramAsrEvent {
                                    kind: if final_result { "final" } else { "delta" }.to_string(),
                                    item_id,
                                    text: combined_text,
                                    start_ms: parsed.event.start_ms,
                                    end_ms: parsed.event.end_ms,
                                });
                                if final_result {
                                    pending_item_id = None;
                                    pending_text.clear();
                                    if finishing {
                                        if let Some(waiter) = finish_waiter.take() { let _ = waiter.send(Ok(())); }
                                        break;
                                    }
                                }
                            }
                        }
                        Ok(Message::Ping(data)) => { let _ = writer.send(Message::Pong(data)).await; }
                        Ok(Message::Close(_)) | Err(_) => {
                            if let Some(waiter) = finish_waiter.take() { let _ = waiter.send(Err("Deepgram 实时转写连接已关闭".to_string())); }
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        let _ = writer.close().await;
        let state = app.state::<DeepgramAsrState>();
        let mut session = state.session.lock().await;
        if session.as_ref().is_some_and(|current| current.id == id) {
            session.take();
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn send_deepgram_asr_audio(
    state: State<'_, DeepgramAsrState>,
    audio_base64: String,
) -> Result<(), String> {
    let audio = BASE64_STANDARD
        .decode(audio_base64)
        .map_err(|error| format!("课堂音频编码无效：{error}"))?;
    if audio.is_empty() {
        return Ok(());
    }
    let command_tx = state
        .session
        .lock()
        .await
        .as_ref()
        .map(|session| session.command_tx.clone())
        .ok_or_else(|| "Deepgram 实时转写尚未启动".to_string())?;
    command_tx
        .send(Command::Audio(audio))
        .await
        .map_err(|_| "Deepgram 音频通道已关闭".to_string())
}

#[tauri::command]
pub async fn finish_deepgram_asr(state: State<'_, DeepgramAsrState>) -> Result<(), String> {
    let session = state.session.lock().await.take();
    let Some(session) = session else {
        return Ok(());
    };
    let (finish_tx, finish_rx) = oneshot::channel();
    session
        .command_tx
        .send(Command::Finish(finish_tx))
        .await
        .map_err(|_| "Deepgram 音频通道已关闭".to_string())?;
    tokio::time::timeout(Duration::from_secs(8), finish_rx)
        .await
        .map_err(|_| "等待 Deepgram 结束超时".to_string())?
        .map_err(|_| "Deepgram 结束响应已丢失".to_string())?
}
