use base64::prelude::*;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, Message},
};
use uuid::Uuid;

const ALIBABA_ASR_BEIJING_URL: &str = "wss://dashscope.aliyuncs.com/api-ws/v1/inference";
const ALIBABA_ASR_SINGAPORE_URL: &str = "wss://dashscope-intl.aliyuncs.com/api-ws/v1/inference";
const QWEN_AUDIO_STREAMING_MODEL: &str = "qwen-audio-3.0-asr-flash-streaming";
const QWEN3_ASR_REALTIME_MODEL: &str = "qwen3-asr-flash-realtime-2026-02-10";
const LEGACY_QWEN3_ASR_REALTIME_MODEL: &str = "qwen3-asr-flash-realtime";
const PARAFORMER_MODEL: &str = "paraformer-realtime-v2";
const SENTENCE_SILENCE_MS: i64 = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AsrProtocol {
    Task,
    QwenRealtime,
}

struct ModelConnection {
    endpoint: String,
    key_provider: &'static str,
    protocol: AsrProtocol,
}

fn model_connection(model: &str, workspace_id: &str) -> Result<ModelConnection, String> {
    match model {
        QWEN_AUDIO_STREAMING_MODEL => Ok(ModelConnection {
            endpoint: if workspace_id.trim().is_empty() {
                ALIBABA_ASR_SINGAPORE_URL.to_string()
            } else {
                format!(
                    "wss://{}/api-ws/v1/inference",
                    super::alibaba_workspace_host(workspace_id)?
                )
            },
            key_provider: "alibaba",
            protocol: AsrProtocol::Task,
        }),
        QWEN3_ASR_REALTIME_MODEL | LEGACY_QWEN3_ASR_REALTIME_MODEL => {
            let base = if workspace_id.trim().is_empty() {
                "wss://dashscope-intl.aliyuncs.com".to_string()
            } else {
                format!("wss://{}", super::alibaba_workspace_host(workspace_id)?)
            };
            Ok(ModelConnection {
                endpoint: format!("{base}/api-ws/v1/realtime?model={model}"),
                key_provider: "alibaba",
                protocol: AsrProtocol::QwenRealtime,
            })
        }
        PARAFORMER_MODEL => Ok(ModelConnection {
            endpoint: ALIBABA_ASR_BEIJING_URL.to_string(),
            key_provider: "alibaba-asr",
            protocol: AsrProtocol::Task,
        }),
        _ => Err("当前不支持所选阿里实时转写模型".to_string()),
    }
}

pub struct AlibabaAsrState {
    session: Mutex<Option<AlibabaAsrSession>>,
}

impl Default for AlibabaAsrState {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
        }
    }
}

struct AlibabaAsrSession {
    task_id: String,
    command_tx: mpsc::Sender<AsrCommand>,
}

enum AsrCommand {
    Audio(Vec<u8>),
    Finish(oneshot::Sender<Result<(), String>>),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAlibabaAsrRequest {
    model: String,
    elapsed_offset_ms: i64,
    hotwords: Vec<AsrHotword>,
    workspace_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AsrHotword {
    text: String,
    weight: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AlibabaAsrEvent {
    kind: String,
    item_id: String,
    text: String,
    start_ms: i64,
    end_ms: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
struct SentenceResult {
    text: String,
    begin_time: i64,
    end_time: Option<i64>,
    sentence_end: bool,
    heartbeat: bool,
    sentence_id: Option<i64>,
}

fn event_name(value: &Value) -> Option<&str> {
    value.get("header")?.get("event")?.as_str()
}

fn sentence_result(value: &Value) -> Option<SentenceResult> {
    if event_name(value) != Some("result-generated") {
        return None;
    }
    let sentence = value.get("payload")?.get("output")?.get("sentence")?;
    Some(SentenceResult {
        text: sentence.get("text")?.as_str()?.trim().to_string(),
        begin_time: sentence
            .get("begin_time")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        end_time: sentence.get("end_time").and_then(Value::as_i64),
        sentence_end: sentence
            .get("sentence_end")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        heartbeat: sentence
            .get("heartbeat")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        sentence_id: sentence.get("sentence_id").and_then(Value::as_i64),
    })
}

fn valid_hotword(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if text.is_ascii() {
        text.split_whitespace().count() <= 7
    } else {
        text.chars().count() <= 15
    }
}

fn hotword_weight(weight: i64) -> i64 {
    if weight == 50 {
        50
    } else {
        weight.clamp(1, 5)
    }
}

fn instant_vocabulary(hotwords: &[AsrHotword]) -> Value {
    let mut vocabulary = Map::new();
    let mut seen = HashSet::new();
    for hotword in hotwords {
        let text = hotword.text.trim();
        let key = text.to_lowercase();
        if !valid_hotword(text) || !seen.insert(key) {
            continue;
        }
        vocabulary.insert(
            text.to_string(),
            Value::from(hotword_weight(hotword.weight)),
        );
        if vocabulary.len() == 2_000 {
            break;
        }
    }
    Value::Object(vocabulary)
}

fn recognition_context(hotwords: &[AsrHotword]) -> Value {
    let mut text = String::new();
    let mut seen = HashSet::new();
    let mut ranked = hotwords.iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        hotword_weight(right.weight)
            .cmp(&hotword_weight(left.weight))
            .then_with(|| left.text.chars().count().cmp(&right.text.chars().count()))
    });
    for hotword in ranked {
        let word = hotword.text.trim();
        let key = word.to_lowercase();
        if !valid_hotword(word) || !seen.insert(key) {
            continue;
        }
        let separator = if text.is_empty() { "" } else { ", " };
        if text.chars().count() + separator.chars().count() + word.chars().count() > 400 {
            break;
        }
        text.push_str(separator);
        text.push_str(word);
    }
    if text.is_empty() {
        json!({})
    } else {
        json!({
            "context": [{
                "role": "user",
                "content": [{"type": "input_text", "text": text}]
            }]
        })
    }
}

fn qwen_realtime_corpus(hotwords: &[AsrHotword]) -> String {
    let mut terms = String::new();
    let mut seen = HashSet::new();
    let mut ranked = hotwords.iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        hotword_weight(right.weight)
            .cmp(&hotword_weight(left.weight))
            .then_with(|| left.text.chars().count().cmp(&right.text.chars().count()))
    });
    for hotword in ranked {
        let term = hotword.text.trim();
        let key = term.to_lowercase();
        if !valid_hotword(term) || !seen.insert(key) {
            continue;
        }
        let separator = if terms.is_empty() { "" } else { ", " };
        if terms.chars().count() + separator.chars().count() + term.chars().count() > 12_000 {
            break;
        }
        terms.push_str(separator);
        terms.push_str(term);
    }
    if terms.is_empty() {
        "An English-language university lecture in Singapore.".to_string()
    } else {
        format!(
            "An English-language university lecture in Singapore. Important course terminology: {terms}."
        )
    }
}

fn qwen_realtime_session_update(hotwords: &[AsrHotword]) -> Value {
    json!({
        "event_id": format!("event_{}", Uuid::new_v4()),
        "type": "session.update",
        "session": {
            "input_audio_format": "pcm",
            "sample_rate": 16000,
            "input_audio_transcription": {
                "language": "en",
                "corpus": {"text": qwen_realtime_corpus(hotwords)}
            },
            "turn_detection": {
                "type": "server_vad",
                "threshold": 0.2,
                "silence_duration_ms": SENTENCE_SILENCE_MS
            }
        }
    })
}

fn qwen_realtime_audio(audio: &[u8]) -> Value {
    json!({
        "event_id": format!("event_{}", Uuid::new_v4()),
        "type": "input_audio_buffer.append",
        "audio": BASE64_STANDARD.encode(audio)
    })
}

fn qwen_realtime_finish() -> Value {
    json!({
        "event_id": format!("event_{}", Uuid::new_v4()),
        "type": "session.finish"
    })
}

fn run_task(task_id: &str, model: &str, hotwords: &[AsrHotword]) -> Value {
    let mut parameters = json!({
        "format": "pcm",
        "sample_rate": 16000,
        "language_hints": ["en"],
        "semantic_punctuation_enabled": false,
        "max_sentence_silence": SENTENCE_SILENCE_MS,
        "multi_threshold_mode_enabled": false,
        "heartbeat": true
    });
    let mut input = json!({});
    if model == QWEN_AUDIO_STREAMING_MODEL {
        parameters["vocabulary"] = instant_vocabulary(hotwords);
        input = recognition_context(hotwords);
    } else {
        parameters["punctuation_prediction_enabled"] = Value::Bool(true);
        parameters["inverse_text_normalization_enabled"] = Value::Bool(true);
        parameters["disfluency_removal_enabled"] = Value::Bool(false);
    }

    json!({
        "header": {
            "action": "run-task",
            "task_id": task_id,
            "streaming": "duplex"
        },
        "payload": {
            "task_group": "audio",
            "task": "asr",
            "function": "recognition",
            "model": model,
            "parameters": parameters,
            "input": input
        }
    })
}

fn finish_task(task_id: &str) -> Value {
    json!({
        "header": {
            "action": "finish-task",
            "task_id": task_id,
            "streaming": "duplex"
        },
        "payload": {"input": {}}
    })
}

fn emit_asr_error(app: &AppHandle, task_id: &str, message: impl Into<String>) {
    let _ = app.emit(
        "alibaba-asr-event",
        AlibabaAsrEvent {
            kind: "error".to_string(),
            item_id: task_id.to_string(),
            text: message.into(),
            start_ms: 0,
            end_ms: None,
        },
    );
}

#[tauri::command]
pub async fn start_alibaba_asr(
    app: AppHandle,
    state: State<'_, AlibabaAsrState>,
    request: StartAlibabaAsrRequest,
) -> Result<(), String> {
    let connection = model_connection(&request.model, &request.workspace_id)?;
    if state.session.lock().await.is_some() {
        return Err("阿里实时转写会话已经启动".to_string());
    }

    let api_key = super::read_provider_api_key(connection.key_provider)?;
    let mut websocket_request = connection
        .endpoint
        .as_str()
        .into_client_request()
        .map_err(|error| format!("无法创建阿里实时转写请求：{error}"))?;
    let mut authorization = HeaderValue::from_str(&format!("Bearer {api_key}"))
        .map_err(|error| format!("阿里实时转写授权信息无效：{error}"))?;
    authorization.set_sensitive(true);
    websocket_request
        .headers_mut()
        .insert("Authorization", authorization);
    websocket_request.headers_mut().insert(
        "User-Agent",
        HeaderValue::from_static("nus-lecture-assistant/0.12.12"),
    );
    if connection.protocol == AsrProtocol::QwenRealtime {
        websocket_request
            .headers_mut()
            .insert("OpenAI-Beta", HeaderValue::from_static("realtime=v1"));
    }
    let (websocket, _) = connect_async(websocket_request)
        .await
        .map_err(|error| format!("无法连接阿里实时转写：{error}"))?;
    let (mut writer, mut reader) = websocket.split();
    let task_id = Uuid::new_v4().to_string();

    let start_event = match connection.protocol {
        AsrProtocol::Task => run_task(&task_id, &request.model, &request.hotwords),
        AsrProtocol::QwenRealtime => qwen_realtime_session_update(&request.hotwords),
    };
    writer
        .send(Message::Text(start_event.to_string().into()))
        .await
        .map_err(|error| format!("无法启动阿里实时转写任务：{error}"))?;

    let protocol = connection.protocol;
    tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(message) = reader.next().await {
            let message = message.map_err(|error| format!("阿里实时转写启动失败：{error}"))?;
            if let Message::Text(text) = message {
                let value: Value = serde_json::from_str(text.as_str())
                    .map_err(|error| format!("阿里实时转写事件格式无效：{error}"))?;
                match protocol {
                    AsrProtocol::Task => match event_name(&value) {
                        Some("task-started") => return Ok(()),
                        Some("task-failed") => {
                            let message = value["header"]["error_message"]
                                .as_str()
                                .unwrap_or("任务启动失败");
                            return Err(format!("阿里实时转写启动失败：{message}"));
                        }
                        _ => {}
                    },
                    AsrProtocol::QwenRealtime => match value["type"].as_str() {
                        Some("session.updated") => return Ok(()),
                        Some("error") => {
                            let message =
                                value["error"]["message"].as_str().unwrap_or("会话配置失败");
                            return Err(format!("阿里实时转写启动失败：{message}"));
                        }
                        _ => {}
                    },
                }
            }
        }
        Err("阿里实时转写连接在启动前关闭".to_string())
    })
    .await
    .map_err(|_| "等待阿里实时转写启动超时".to_string())??;

    let (command_tx, mut command_rx) = mpsc::channel::<AsrCommand>(24);
    *state.session.lock().await = Some(AlibabaAsrSession {
        task_id: task_id.clone(),
        command_tx: command_tx.clone(),
    });

    tauri::async_runtime::spawn(async move {
        let mut finish_waiter: Option<oneshot::Sender<Result<(), String>>> = None;
        let mut qwen_item_starts = HashMap::<String, i64>::new();
        loop {
            tokio::select! {
                command = command_rx.recv() => {
                    match command {
                        Some(AsrCommand::Audio(audio)) => {
                            let message = match protocol {
                                AsrProtocol::Task => Message::Binary(audio.into()),
                                AsrProtocol::QwenRealtime => Message::Text(
                                    qwen_realtime_audio(&audio).to_string().into(),
                                ),
                            };
                            if let Err(error) = writer.send(message).await {
                                emit_asr_error(&app, &task_id, format!("发送课堂音频失败：{error}"));
                                break;
                            }
                        }
                        Some(AsrCommand::Finish(waiter)) => {
                            finish_waiter = Some(waiter);
                            let finish_event = match protocol {
                                AsrProtocol::Task => finish_task(&task_id),
                                AsrProtocol::QwenRealtime => qwen_realtime_finish(),
                            };
                            if let Err(error) = writer.send(Message::Text(finish_event.to_string().into())).await {
                                if let Some(waiter) = finish_waiter.take() {
                                    let _ = waiter.send(Err(format!("结束阿里转写失败：{error}")));
                                }
                                break;
                            }
                        }
                        None => break,
                    }
                }
                message = reader.next() => {
                    let Some(message) = message else {
                        if let Some(waiter) = finish_waiter.take() {
                            let _ = waiter.send(Err("阿里实时转写连接提前关闭".to_string()));
                        }
                        break;
                    };
                    match message {
                        Ok(Message::Text(text)) => {
                            let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else {
                                continue;
                            };
                            match protocol {
                                AsrProtocol::Task => {
                                    if let Some(sentence) = sentence_result(&value) {
                                        if sentence.heartbeat || sentence.text.is_empty() {
                                            continue;
                                        }
                                        let item_id = format!(
                                            "{task_id}:{}",
                                            sentence.sentence_id.unwrap_or(sentence.begin_time)
                                        );
                                        let _ = app.emit(
                                            "alibaba-asr-event",
                                            AlibabaAsrEvent {
                                                kind: if sentence.sentence_end { "final" } else { "delta" }.to_string(),
                                                item_id,
                                                text: sentence.text,
                                                start_ms: request.elapsed_offset_ms + sentence.begin_time,
                                                end_ms: sentence.end_time.map(|value| request.elapsed_offset_ms + value),
                                            },
                                        );
                                    }
                                    match event_name(&value) {
                                        Some("task-finished") => {
                                            if let Some(waiter) = finish_waiter.take() {
                                                let _ = waiter.send(Ok(()));
                                            }
                                            break;
                                        }
                                        Some("task-failed") => {
                                            let message = value["header"]["error_message"]
                                                .as_str()
                                                .unwrap_or("未知错误")
                                                .to_string();
                                            emit_asr_error(&app, &task_id, &message);
                                            if let Some(waiter) = finish_waiter.take() {
                                                let _ = waiter.send(Err(message));
                                            }
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                                AsrProtocol::QwenRealtime => {
                                    let event_type = value["type"].as_str().unwrap_or_default();
                                    match event_type {
                                        "input_audio_buffer.speech_started" => {
                                            if let Some(item_id) = value["item_id"].as_str() {
                                                let start_ms = value["audio_start_ms"].as_i64().unwrap_or_default();
                                                qwen_item_starts.insert(item_id.to_string(), start_ms);
                                            }
                                        }
                                        "conversation.item.input_audio_transcription.text" => {
                                            let Some(item_id) = value["item_id"].as_str() else {
                                                continue;
                                            };
                                            let preview = format!(
                                                "{}{}",
                                                value["text"].as_str().unwrap_or_default(),
                                                value["stash"].as_str().unwrap_or_default()
                                            );
                                            if preview.trim().is_empty() {
                                                continue;
                                            }
                                            let start_ms = qwen_item_starts.get(item_id).copied().unwrap_or_default();
                                            let _ = app.emit(
                                                "alibaba-asr-event",
                                                AlibabaAsrEvent {
                                                    kind: "delta".to_string(),
                                                    item_id: item_id.to_string(),
                                                    text: preview,
                                                    start_ms: request.elapsed_offset_ms + start_ms,
                                                    end_ms: None,
                                                },
                                            );
                                        }
                                        "conversation.item.input_audio_transcription.completed" => {
                                            let Some(item_id) = value["item_id"].as_str() else {
                                                continue;
                                            };
                                            let transcript = value["transcript"]
                                                .as_str()
                                                .unwrap_or_default()
                                                .trim();
                                            if transcript.is_empty() {
                                                continue;
                                            }
                                            let start_ms = qwen_item_starts.remove(item_id).unwrap_or_default();
                                            let _ = app.emit(
                                                "alibaba-asr-event",
                                                AlibabaAsrEvent {
                                                    kind: "final".to_string(),
                                                    item_id: item_id.to_string(),
                                                    text: transcript.to_string(),
                                                    start_ms: request.elapsed_offset_ms + start_ms,
                                                    end_ms: None,
                                                },
                                            );
                                        }
                                        "session.finished" => {
                                            if let Some(waiter) = finish_waiter.take() {
                                                let _ = waiter.send(Ok(()));
                                            }
                                            break;
                                        }
                                        "conversation.item.input_audio_transcription.failed" | "error" => {
                                            let message = value["error"]["message"]
                                                .as_str()
                                                .unwrap_or("Qwen3 实时转写失败")
                                                .to_string();
                                            emit_asr_error(&app, &task_id, &message);
                                            if let Some(waiter) = finish_waiter.take() {
                                                let _ = waiter.send(Err(message));
                                            }
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        Ok(Message::Ping(data)) => {
                            let _ = writer.send(Message::Pong(data)).await;
                        }
                        Ok(Message::Close(_)) | Err(_) => {
                            if let Some(waiter) = finish_waiter.take() {
                                let _ = waiter.send(Err("阿里实时转写连接已关闭".to_string()));
                            }
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        let _ = writer.close().await;
        let state = app.state::<AlibabaAsrState>();
        let mut session = state.session.lock().await;
        if session
            .as_ref()
            .is_some_and(|current| current.task_id == task_id)
        {
            session.take();
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn send_alibaba_asr_audio(
    state: State<'_, AlibabaAsrState>,
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
        .ok_or_else(|| "阿里实时转写尚未启动".to_string())?;
    command_tx
        .send(AsrCommand::Audio(audio))
        .await
        .map_err(|_| "阿里实时转写音频通道已关闭".to_string())
}

#[tauri::command]
pub async fn finish_alibaba_asr(state: State<'_, AlibabaAsrState>) -> Result<(), String> {
    let finish_rx = {
        let session = state.session.lock().await;
        let Some(session) = session.as_ref() else {
            return Ok(());
        };
        let (finish_tx, finish_rx) = oneshot::channel();
        session
            .command_tx
            .send(AsrCommand::Finish(finish_tx))
            .await
            .map_err(|_| "阿里实时转写会话已关闭".to_string())?;
        finish_rx
    };
    let result = tokio::time::timeout(Duration::from_secs(12), finish_rx)
        .await
        .map_err(|_| "等待阿里实时转写结束超时".to_string())?
        .map_err(|_| "阿里实时转写结束信号丢失".to_string())?;
    state.session.lock().await.take();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_balanced_english_pcm_task() {
        let payload = run_task("task-id", PARAFORMER_MODEL, &[]);
        assert_eq!(payload["payload"]["parameters"]["format"], "pcm");
        assert_eq!(payload["payload"]["parameters"]["sample_rate"], 16000);
        assert_eq!(payload["payload"]["parameters"]["language_hints"][0], "en");
        assert_eq!(
            payload["payload"]["parameters"]["max_sentence_silence"],
            SENTENCE_SILENCE_MS
        );
        assert_eq!(
            payload["payload"]["parameters"]["multi_threshold_mode_enabled"],
            false
        );
    }

    #[test]
    fn routes_qwen_streaming_to_singapore_with_instant_hotwords() {
        let connection = model_connection(QWEN_AUDIO_STREAMING_MODEL, "").expect("qwen model");
        assert_eq!(connection.endpoint, ALIBABA_ASR_SINGAPORE_URL);
        let dedicated = model_connection(QWEN_AUDIO_STREAMING_MODEL, "llm-classroom")
            .expect("dedicated qwen model");
        assert_eq!(
            dedicated.endpoint,
            "wss://llm-classroom.ap-southeast-1.maas.aliyuncs.com/api-ws/v1/inference"
        );
        assert_eq!(connection.key_provider, "alibaba");
        assert_eq!(connection.protocol, AsrProtocol::Task);

        let payload = run_task(
            "task-id",
            QWEN_AUDIO_STREAMING_MODEL,
            &[
                AsrHotword {
                    text: "Singtel".to_string(),
                    weight: 5,
                },
                AsrHotword {
                    text: "StarHub".to_string(),
                    weight: 4,
                },
            ],
        );
        assert_eq!(payload["payload"]["parameters"]["vocabulary"]["Singtel"], 5);
        assert_eq!(payload["payload"]["parameters"]["vocabulary"]["StarHub"], 4);
        assert_eq!(
            payload["payload"]["input"]["context"][0]["content"][0]["text"],
            "Singtel, StarHub"
        );
    }

    #[test]
    fn builds_qwen3_accent_optimized_realtime_session() {
        let connection = model_connection(QWEN3_ASR_REALTIME_MODEL, "").expect("qwen3 model");
        assert_eq!(
            connection.endpoint,
            "wss://dashscope-intl.aliyuncs.com/api-ws/v1/realtime?model=qwen3-asr-flash-realtime-2026-02-10"
        );
        assert_eq!(connection.protocol, AsrProtocol::QwenRealtime);
        let dedicated = model_connection(QWEN3_ASR_REALTIME_MODEL, "llm-classroom")
            .expect("dedicated qwen3 model");
        assert_eq!(
            dedicated.endpoint,
            "wss://llm-classroom.ap-southeast-1.maas.aliyuncs.com/api-ws/v1/realtime?model=qwen3-asr-flash-realtime-2026-02-10"
        );

        let legacy =
            model_connection(LEGACY_QWEN3_ASR_REALTIME_MODEL, "").expect("legacy qwen3 model");
        assert!(legacy.endpoint.ends_with("model=qwen3-asr-flash-realtime"));

        let payload = qwen_realtime_session_update(&[
            AsrHotword {
                text: "perceptron".to_string(),
                weight: 5,
            },
            AsrHotword {
                text: "synaptic weights".to_string(),
                weight: 4,
            },
        ]);
        assert_eq!(payload["type"], "session.update");
        assert_eq!(
            payload["session"]["input_audio_transcription"]["language"],
            "en"
        );
        assert_eq!(payload["session"]["turn_detection"]["threshold"], 0.2);
        assert_eq!(
            payload["session"]["turn_detection"]["silence_duration_ms"],
            SENTENCE_SILENCE_MS
        );
        let corpus = payload["session"]["input_audio_transcription"]["corpus"]["text"]
            .as_str()
            .expect("corpus");
        assert!(corpus.contains("perceptron"));
        assert!(corpus.contains("synaptic weights"));

        let audio = qwen_realtime_audio(&[1, 2, 3]);
        assert_eq!(audio["type"], "input_audio_buffer.append");
        assert_eq!(audio["audio"], "AQID");
        assert_eq!(qwen_realtime_finish()["type"], "session.finish");
    }

    #[test]
    fn validates_and_deduplicates_instant_hotwords() {
        let payload = instant_vocabulary(&[
            AsrHotword {
                text: "Singtel".to_string(),
                weight: 9,
            },
            AsrHotword {
                text: "singtel".to_string(),
                weight: 2,
            },
            AsrHotword {
                text: "one two three four five six seven eight".to_string(),
                weight: 4,
            },
        ]);
        assert_eq!(payload["Singtel"], 5);
        assert!(payload.get("singtel").is_none());
        assert_eq!(payload.as_object().map(Map::len), Some(1));
    }

    #[test]
    fn parses_partial_and_final_sentence_fields() {
        let event = json!({
            "header": {"event": "result-generated"},
            "payload": {"output": {"sentence": {
                "begin_time": 170,
                "end_time": 920,
                "text": "Price elasticity",
                "heartbeat": false,
                "sentence_end": true
            }}}
        });
        assert_eq!(
            sentence_result(&event),
            Some(SentenceResult {
                text: "Price elasticity".to_string(),
                begin_time: 170,
                end_time: Some(920),
                sentence_end: true,
                heartbeat: false,
                sentence_id: None,
            })
        );
    }
}
