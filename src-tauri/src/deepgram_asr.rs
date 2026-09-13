use base64::prelude::*;
use futures_util::{Sink, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::Instant;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig, Message,
    },
};
use uuid::Uuid;

const EVENT_NAME: &str = "deepgram-asr-event";
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(3);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const FINISH_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_AUDIO_BYTES: usize = 128 * 1024;
const MAX_SERVER_MESSAGE_BYTES: usize = 1024 * 1024;

fn websocket_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_SERVER_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_SERVER_MESSAGE_BYTES))
}

#[derive(Default)]
pub struct DeepgramAsrState {
    session: Mutex<Option<DeepgramAsrSession>>,
}

struct DeepgramAsrSession {
    task_id: String,
    command_tx: mpsc::Sender<AsrCommand>,
    cancel_tx: Option<oneshot::Sender<()>>,
    finishing: bool,
}

enum AsrCommand {
    Audio(Vec<u8>),
    Finish(oneshot::Sender<Result<(), String>>),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDeepgramAsrRequest {
    model: String,
    elapsed_offset_ms: i64,
    #[serde(default)]
    hotwords: Vec<AsrHotword>,
}

#[derive(Deserialize)]
struct AsrHotword {
    text: String,
    #[serde(default)]
    weight: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct DeepgramAsrEvent {
    kind: String,
    item_id: String,
    text: String,
    start_ms: i64,
    end_ms: Option<i64>,
}

fn listen_endpoint(model: &str, hotwords: &[AsrHotword]) -> Result<String, String> {
    if !matches!(model, "nova-3" | "nova-2") {
        return Err("当前不支持所选 Deepgram 实时转写模型".to_string());
    }
    let mut url = reqwest::Url::parse("wss://api.deepgram.com/v1/listen")
        .map_err(|_| "Deepgram 服务地址无效".to_string())?;
    let mut query = url.query_pairs_mut();
    query.extend_pairs([
        ("model", model),
        ("language", "en"),
        ("encoding", "linear16"),
        ("sample_rate", "16000"),
        ("channels", "1"),
        ("interim_results", "true"),
        ("endpointing", "400"),
        ("utterance_end_ms", "1000"),
        ("smart_format", "true"),
        ("punctuate", "true"),
    ]);
    let mut ranked = hotwords.iter().collect::<Vec<_>>();
    ranked.sort_by_key(|word| std::cmp::Reverse(word.weight));
    let mut seen = HashSet::new();
    let mut bytes = 0;
    for word in ranked {
        let term = word.text.trim();
        if term.is_empty()
            || term.len() > 80
            || term.chars().any(char::is_control)
            || (model == "nova-2" && term.contains(':'))
            || seen.contains(&term.to_lowercase())
        {
            continue;
        }
        // Byte budgeting is conservative for Nova-3's 500-token keyterm limit.
        let cost = term.len() + 1;
        if bytes + cost > 500 {
            continue;
        }
        if model == "nova-3" {
            query.append_pair("keyterm", term);
        } else {
            query.append_pair("keywords", &format!("{term}:{}", word.weight.clamp(1, 5)));
        }
        seen.insert(term.to_lowercase());
        bytes += cost;
        if seen.len() == 100 {
            break;
        }
    }
    drop(query);
    Ok(url.into())
}

fn milliseconds(value: Option<&Value>) -> Option<i64> {
    value
        .and_then(Value::as_f64)
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| (seconds * 1000.0).round() as i64)
}

struct TranscriptAccumulator {
    task_id: String,
    offset_ms: i64,
    sequence: u64,
    committed: String,
    interim: String,
    start_ms: Option<i64>,
    final_end_ms: Option<i64>,
    last_word_end_ms: Option<i64>,
    final_boundary_ms: i64,
    recent_finals: VecDeque<(i64, i64, String)>,
}

impl TranscriptAccumulator {
    fn new(task_id: String, offset_ms: i64) -> Self {
        Self {
            task_id,
            offset_ms,
            sequence: 0,
            committed: String::new(),
            interim: String::new(),
            start_ms: None,
            final_end_ms: None,
            last_word_end_ms: None,
            final_boundary_ms: -1,
            recent_finals: VecDeque::new(),
        }
    }

    fn event(&self, kind: &str, text: String, end_ms: Option<i64>) -> DeepgramAsrEvent {
        DeepgramAsrEvent {
            kind: kind.to_string(),
            item_id: format!("{}:{}", self.task_id, self.sequence),
            text,
            start_ms: self.offset_ms.saturating_add(self.start_ms.unwrap_or(0)),
            end_ms: end_ms.map(|end| self.offset_ms.saturating_add(end)),
        }
    }

    fn preview(&self) -> Option<DeepgramAsrEvent> {
        let text = [self.committed.as_str(), self.interim.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        (!text.is_empty()).then(|| self.event("delta", text, None))
    }

    fn flush(&mut self) -> Option<DeepgramAsrEvent> {
        if self.committed.is_empty() {
            return None;
        }
        let event = self.event("final", self.committed.clone(), self.final_end_ms);
        self.final_boundary_ms = self.final_end_ms.unwrap_or(self.final_boundary_ms);
        self.committed.clear();
        self.interim.clear();
        self.start_ms = None;
        self.final_end_ms = None;
        self.last_word_end_ms = None;
        self.sequence += 1;
        Some(event)
    }

    fn accept(&mut self, value: &Value) -> Option<DeepgramAsrEvent> {
        match value["type"].as_str()? {
            "Results" => {
                let transcript = value["channel"]["alternatives"][0]["transcript"]
                    .as_str()
                    .unwrap_or_default()
                    .trim();
                let is_final = value["is_final"].as_bool().unwrap_or(false);
                let speech_final = value["speech_final"].as_bool().unwrap_or(false);
                let from_finalize = value["from_finalize"].as_bool().unwrap_or(false);
                let chunk_start = milliseconds(value.get("start")).unwrap_or_default();
                let chunk_end = chunk_start
                    .saturating_add(milliseconds(value.get("duration")).unwrap_or_default());
                let words = value["channel"]["alternatives"][0]["words"].as_array();
                let speech_start = words
                    .and_then(|words| words.first())
                    .and_then(|word| milliseconds(word.get("start")))
                    .unwrap_or(chunk_start);
                let word_end = words
                    .and_then(|words| words.last())
                    .and_then(|word| milliseconds(word.get("end")))
                    .unwrap_or(chunk_end);

                if !transcript.is_empty() {
                    // Server retries must not append a final fragment or resurrect an old item.
                    let signature = (chunk_start, chunk_end, transcript.to_string());
                    if chunk_end <= self.final_boundary_ms {
                        return None;
                    }
                    if is_final && self.recent_finals.contains(&signature) {
                        return if speech_final || from_finalize {
                            self.flush()
                        } else {
                            None
                        };
                    }
                    if !is_final && self.final_end_ms.is_some_and(|end| chunk_end <= end) {
                        return None;
                    }
                    self.start_ms.get_or_insert(speech_start);
                    if is_final {
                        self.recent_finals.push_back(signature);
                        if self.recent_finals.len() > 256 {
                            self.recent_finals.pop_front();
                        }
                        if !self.committed.is_empty() {
                            self.committed.push(' ');
                        }
                        self.committed.push_str(transcript);
                        self.interim.clear();
                        self.final_end_ms = Some(chunk_end);
                        self.last_word_end_ms = Some(word_end);
                    } else {
                        self.interim = transcript.to_string();
                    }
                } else if is_final {
                    self.interim.clear();
                }

                if is_final && (speech_final || from_finalize) {
                    self.flush()
                } else {
                    self.preview()
                }
            }
            "UtteranceEnd" => {
                let last_word_end = milliseconds(value.get("last_word_end"))?;
                if self
                    .last_word_end_ms
                    .is_some_and(|end| last_word_end >= end)
                {
                    self.flush()
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

async fn write_message<S>(writer: &mut S, message: Message) -> Result<(), String>
where
    S: Sink<Message> + Unpin,
{
    tokio::time::timeout(WRITE_TIMEOUT, writer.send(message))
        .await
        .map_err(|_| "向 Deepgram 发送数据超时".to_string())?
        .map_err(|_| "Deepgram 连接已断开，无法发送课堂音频".to_string())
}

#[tauri::command]
pub async fn start_deepgram_asr(
    app: AppHandle,
    state: State<'_, DeepgramAsrState>,
    request: StartDeepgramAsrRequest,
) -> Result<(), String> {
    let endpoint = listen_endpoint(&request.model, &request.hotwords)?;
    let mut session_guard = state.session.lock().await;
    if session_guard.is_some() {
        return Err("Deepgram 实时转写会话已经启动".to_string());
    }
    let api_key = tauri::async_runtime::spawn_blocking(|| super::read_provider_api_key("deepgram"))
        .await
        .map_err(|_| "无法读取 Deepgram API Key".to_string())??;
    if !super::valid_provider_api_key("deepgram", &api_key) {
        return Err("已保存的 Deepgram API Key 格式无效".to_string());
    }
    let mut websocket_request = endpoint
        .as_str()
        .into_client_request()
        .map_err(|_| "无法创建 Deepgram 实时转写请求".to_string())?;
    let mut authorization = HeaderValue::from_str(&format!("Token {api_key}"))
        .map_err(|_| "Deepgram 授权信息无效".to_string())?;
    authorization.set_sensitive(true);
    websocket_request
        .headers_mut()
        .insert("Authorization", authorization);
    let (websocket, _) = tokio::time::timeout(
        Duration::from_secs(15),
        connect_async_with_config(websocket_request, Some(websocket_config()), false),
    )
    .await
    .map_err(|_| "连接 Deepgram 实时转写超时".to_string())?
    .map_err(|error| match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => match response.status().as_u16() {
            401 | 403 => "Deepgram 拒绝授权，请检查 API Key、权限和账户状态".to_string(),
            402 => "Deepgram 账户额度不足".to_string(),
            429 => "Deepgram 请求受限，请稍后重试或检查账户并发限制".to_string(),
            status => format!("Deepgram 连接请求失败（HTTP {status}）"),
        },
        _ => "无法连接 Deepgram，请检查网络后重试".to_string(),
    })?;
    let task_id = Uuid::new_v4().to_string();
    let (command_tx, mut command_rx) = mpsc::channel::<AsrCommand>(24);
    let (cancel_tx, mut cancel_rx) = oneshot::channel();
    *session_guard = Some(DeepgramAsrSession {
        task_id: task_id.clone(),
        command_tx,
        cancel_tx: Some(cancel_tx),
        finishing: false,
    });
    drop(session_guard);

    tauri::async_runtime::spawn(async move {
        let (mut writer, mut reader) = websocket.split();
        let mut transcript =
            TranscriptAccumulator::new(task_id.clone(), request.elapsed_offset_ms.max(0));
        let mut finish_waiter: Option<oneshot::Sender<Result<(), String>>> = None;
        let mut finish_deadline = None;
        let mut keepalive = tokio::time::interval(KEEPALIVE_INTERVAL);
        keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last_audio = Instant::now();
        let outcome = loop {
            // A disabled deadline stays pending while the stream is active.
            let deadline = async {
                if let Some(deadline) = finish_deadline {
                    tokio::time::sleep_until(deadline).await;
                } else {
                    std::future::pending::<()>().await;
                }
            };
            tokio::select! {
                _ = &mut cancel_rx => break Err("Deepgram 转写结束超时，连接已清理".to_string()),
                _ = deadline => break Err("等待 Deepgram 返回最后一句超时".to_string()),
                _ = keepalive.tick(), if finish_waiter.is_none() => {
                    if last_audio.elapsed() >= KEEPALIVE_INTERVAL {
                        if let Err(error) = write_message(&mut writer, Message::Text(r#"{"type":"KeepAlive"}"#.into())).await {
                            break Err(error);
                        }
                    }
                }
                command = command_rx.recv(), if finish_waiter.is_none() => {
                    match command {
                        Some(AsrCommand::Audio(audio)) => {
                            if let Err(error) = write_message(&mut writer, Message::Binary(audio.into())).await {
                                break Err(error);
                            }
                            last_audio = Instant::now();
                        }
                        Some(AsrCommand::Finish(waiter)) => {
                            finish_waiter = Some(waiter);
                            finish_deadline = Some(Instant::now() + FINISH_TIMEOUT);
                            if let Err(error) = write_message(&mut writer, Message::Text(r#"{"type":"CloseStream"}"#.into())).await {
                                break Err(error);
                            }
                        }
                        None => break Err("Deepgram 音频通道已关闭".to_string()),
                    }
                }
                message = reader.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else {
                                break Err("Deepgram 返回了无效的转写数据".to_string());
                            };
                            if let Some(event) = transcript.accept(&value) {
                                let _ = app.emit(EVENT_NAME, event);
                            }
                            match value["type"].as_str() {
                                Some("Error") => break Err("Deepgram 转写失败，请检查账户额度、模型权限和网络".to_string()),
                                // Metadata follows all final Results when CloseStream completes.
                                Some("Metadata") if finish_waiter.is_some() => break Ok(()),
                                _ => {}
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            if let Err(error) = write_message(&mut writer, Message::Pong(data)).await {
                                break Err(error);
                            }
                        }
                        Some(Ok(Message::Close(frame))) => {
                            if finish_waiter.is_some() && frame.as_ref().is_none_or(|frame| frame.code == tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal) {
                                break Ok(());
                            }
                            break Err("Deepgram 实时转写连接意外关闭，请重新开始课堂转写".to_string());
                        }
                        Some(Err(_)) | None => break Err("Deepgram 实时转写连接已断开，请检查网络后重新开始".to_string()),
                        _ => {}
                    }
                }
            }
        };
        if let Some(event) = transcript.flush() {
            let _ = app.emit(EVENT_NAME, event);
        }
        if let Err(error) = &outcome {
            let _ = app.emit(
                EVENT_NAME,
                DeepgramAsrEvent {
                    kind: "error".to_string(),
                    item_id: task_id.clone(),
                    text: error.clone(),
                    start_ms: 0,
                    end_ms: None,
                },
            );
        }
        let _ = tokio::time::timeout(Duration::from_secs(2), writer.close()).await;
        let state = app.state::<DeepgramAsrState>();
        let mut session = state.session.lock().await;
        if session
            .as_ref()
            .is_some_and(|current| current.task_id == task_id)
        {
            session.take();
        }
        drop(session);
        if let Some(waiter) = finish_waiter {
            let _ = waiter.send(outcome);
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn send_deepgram_asr_audio(
    state: State<'_, DeepgramAsrState>,
    audio_base64: String,
) -> Result<(), String> {
    if audio_base64.len() > MAX_AUDIO_BYTES.div_ceil(3) * 4 {
        return Err("课堂音频片段过大".to_string());
    }
    let audio = BASE64_STANDARD
        .decode(audio_base64)
        .map_err(|_| "课堂音频编码无效".to_string())?;
    if audio.is_empty() {
        return Ok(());
    }
    if audio.len() > MAX_AUDIO_BYTES || audio.len() % 2 != 0 {
        return Err("课堂音频必须为 16 位单声道 PCM".to_string());
    }
    let session = state.session.lock().await;
    let current = session
        .as_ref()
        .ok_or_else(|| "Deepgram 实时转写尚未启动".to_string())?;
    if current.finishing {
        return Err("Deepgram 实时转写正在结束".to_string());
    }
    // Hold the lock until queued so Finish cannot overtake accepted audio.
    tokio::time::timeout(
        Duration::from_secs(6),
        current.command_tx.send(AsrCommand::Audio(audio)),
    )
    .await
    .map_err(|_| "Deepgram 音频积压，请检查网络".to_string())?
    .map_err(|_| "Deepgram 音频通道已关闭".to_string())
}

#[tauri::command]
pub async fn finish_deepgram_asr(state: State<'_, DeepgramAsrState>) -> Result<(), String> {
    let mut session = state.session.lock().await;
    let Some(current) = session.as_mut() else {
        return Ok(());
    };
    if current.finishing {
        return Err("Deepgram 实时转写正在结束".to_string());
    }
    current.finishing = true;
    let command_tx = current.command_tx.clone();
    let task_id = current.task_id.clone();
    drop(session);
    let (finish_tx, finish_rx) = oneshot::channel();
    let queued = tokio::time::timeout(
        Duration::from_secs(6),
        command_tx.send(AsrCommand::Finish(finish_tx)),
    )
    .await;
    let outcome = match queued {
        Ok(Ok(())) => match tokio::time::timeout(Duration::from_secs(25), finish_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("Deepgram 实时转写结束信号丢失".to_string()),
            Err(_) => Err("等待 Deepgram 实时转写结束超时".to_string()),
        },
        Ok(Err(_)) => Err("Deepgram 实时转写会话已关闭".to_string()),
        Err(_) => Err("Deepgram 音频积压，无法结束转写".to_string()),
    };
    if outcome.is_err() {
        let mut session = state.session.lock().await;
        if let Some(current) = session
            .as_mut()
            .filter(|current| current.task_id == task_id)
        {
            if let Some(cancel_tx) = current.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn result(text: &str, start: f64, duration: f64, is_final: bool, speech_final: bool) -> Value {
        json!({"type":"Results","start":start,"duration":duration,
            "is_final":is_final,"speech_final":speech_final,
            "channel":{"alternatives":[{"transcript":text}]}})
    }

    #[test]
    fn uses_fixed_nova3_english_pcm_endpoint_and_encodes_keyterms() {
        let url = reqwest::Url::parse(
            &listen_endpoint(
                "nova-3",
                &[
                    AsrHotword {
                        text: "R&D + AI".to_string(),
                        weight: 5,
                    },
                    AsrHotword {
                        text: "r&d + ai".to_string(),
                        weight: 1,
                    },
                ],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(url.host_str(), Some("api.deepgram.com"));
        assert_eq!(url.path(), "/v1/listen");
        let pairs = url.query_pairs().collect::<Vec<_>>();
        for (name, value) in [
            ("encoding", "linear16"),
            ("sample_rate", "16000"),
            ("channels", "1"),
            ("endpointing", "400"),
            ("interim_results", "true"),
            ("utterance_end_ms", "1000"),
            ("language", "en"),
        ] {
            assert!(pairs.iter().any(|pair| pair.0 == name && pair.1 == value));
        }
        assert_eq!(pairs.iter().filter(|pair| pair.0 == "keyterm").count(), 1);
        assert!(pairs
            .iter()
            .any(|pair| pair.0 == "keyterm" && pair.1 == "R&D + AI"));
        assert!(listen_endpoint("nova-3&language=zh", &[]).is_err());
    }

    #[test]
    fn bounds_incoming_websocket_messages() {
        let config = websocket_config();
        assert_eq!(config.max_message_size, Some(MAX_SERVER_MESSAGE_BYTES));
        assert_eq!(config.max_frame_size, Some(MAX_SERVER_MESSAGE_BYTES));
        assert!(!config.accept_unmasked_frames);
    }

    #[test]
    fn bounds_hotword_budget_and_prioritizes_high_weight_terms() {
        let mut hotwords = (0..200)
            .map(|index| AsrHotword {
                text: format!("term-{index}"),
                weight: 1,
            })
            .collect::<Vec<_>>();
        hotwords.push(AsrHotword {
            text: "priority".to_string(),
            weight: 50,
        });
        let url = reqwest::Url::parse(&listen_endpoint("nova-3", &hotwords).unwrap()).unwrap();
        let terms = url
            .query_pairs()
            .filter(|pair| pair.0 == "keyterm")
            .map(|pair| pair.1.into_owned())
            .collect::<Vec<_>>();
        assert_eq!(terms[0], "priority");
        assert!(terms.len() <= 100);
        assert!(terms.iter().map(|term| term.len() + 1).sum::<usize>() <= 500);
    }

    #[test]
    fn nova2_uses_keyword_boosting_instead_of_nova3_keyterms() {
        let url = reqwest::Url::parse(
            &listen_endpoint(
                "nova-2",
                &[
                    AsrHotword {
                        text: "NUS".to_string(),
                        weight: 50,
                    },
                    AsrHotword {
                        text: "term:unsafe-boost".to_string(),
                        weight: 5,
                    },
                ],
            )
            .unwrap(),
        )
        .unwrap();
        let pairs = url.query_pairs().collect::<Vec<_>>();
        assert!(pairs
            .iter()
            .any(|pair| pair.0 == "model" && pair.1 == "nova-2"));
        assert!(pairs
            .iter()
            .any(|pair| pair.0 == "keywords" && pair.1 == "NUS:5"));
        assert_eq!(pairs.iter().filter(|pair| pair.0 == "keywords").count(), 1);
        assert!(!pairs.iter().any(|pair| pair.0 == "keyterm"));
    }

    #[test]
    fn replaces_interims_and_combines_final_fragments_until_speech_final() {
        let mut transcript = TranscriptAccumulator::new("task".to_string(), 5000);
        let first = transcript
            .accept(&result("Price", 0.0, 0.5, false, false))
            .unwrap();
        let revised = transcript
            .accept(&result("Price elasticity", 0.0, 1.0, false, false))
            .unwrap();
        assert_eq!(first.item_id, revised.item_id);
        assert_eq!(revised.text, "Price elasticity");
        let fragment = result("Price elasticity", 0.0, 1.0, true, false);
        assert_eq!(transcript.accept(&fragment).unwrap().kind, "delta");
        assert!(transcript.accept(&fragment).is_none());
        assert_eq!(
            transcript
                .accept(&result("measures", 1.0, 0.4, false, false))
                .unwrap()
                .text,
            "Price elasticity measures"
        );
        let final_event = transcript
            .accept(&result("measures demand.", 1.0, 1.5, true, true))
            .unwrap();
        assert_eq!(final_event.kind, "final");
        assert_eq!(final_event.item_id, first.item_id);
        assert_eq!(final_event.text, "Price elasticity measures demand.");
        assert_eq!(final_event.start_ms, 5000);
        assert_eq!(final_event.end_ms, Some(7500));
        assert!(transcript.accept(&fragment).is_none());
        let next = transcript
            .accept(&result("Next sentence", 2.5, 1.0, true, true))
            .unwrap();
        assert_ne!(next.item_id, final_event.item_id);
    }

    #[test]
    fn empty_endpoint_result_flushes_committed_text_only_once() {
        let mut transcript = TranscriptAccumulator::new("task".to_string(), 0);
        transcript.accept(&result("Final phrase", 0.0, 1.0, true, false));
        let endpoint = result("", 1.0, 0.4, true, true);
        assert_eq!(transcript.accept(&endpoint).unwrap().text, "Final phrase");
        assert!(transcript.accept(&endpoint).is_none());
        assert!(transcript.flush().is_none());
    }

    #[test]
    fn repeated_final_can_supply_boundary_without_duplicating_text() {
        let mut transcript = TranscriptAccumulator::new("task".to_string(), 0);
        let fragment = result("A complete sentence.", 0.0, 1.0, true, false);
        transcript.accept(&fragment);
        assert!(transcript
            .accept(&result("A complete", 0.0, 0.7, false, false))
            .is_none());
        let mut endpoint = fragment;
        endpoint["speech_final"] = Value::Bool(true);
        let final_event = transcript.accept(&endpoint).unwrap();
        assert_eq!(final_event.kind, "final");
        assert_eq!(final_event.text, "A complete sentence.");
        assert!(transcript.accept(&endpoint).is_none());
    }

    #[test]
    fn utterance_end_uses_word_end_and_ignores_late_previous_boundary() {
        let mut transcript = TranscriptAccumulator::new("task".to_string(), 100);
        let mut fragment = result("Technical term", 0.0, 2.0, true, false);
        fragment["channel"]["alternatives"][0]["words"] = json!([
            {"word":"Technical","start":0.2,"end":0.8},
            {"word":"term","start":0.9,"end":1.3}
        ]);
        transcript.accept(&fragment);
        let boundary = json!({"type":"UtteranceEnd","last_word_end":1.3});
        let final_event = transcript.accept(&boundary).unwrap();
        assert_eq!(final_event.start_ms, 300);
        assert_eq!(final_event.text, "Technical term");
        transcript.accept(&result("Next phrase", 2.0, 1.0, true, false));
        assert!(transcript.accept(&boundary).is_none());
        assert_eq!(transcript.flush().unwrap().text, "Next phrase");
    }

    #[test]
    fn from_finalize_delivers_tail_before_completion_and_does_not_promote_interim() {
        let mut transcript = TranscriptAccumulator::new("task".to_string(), 0);
        transcript.accept(&result("The last", 0.0, 1.0, true, false));
        transcript.accept(&result("unfinished preview", 1.0, 1.0, false, false));
        let mut tail = result("sentence.", 1.0, 1.0, true, false);
        tail["from_finalize"] = Value::Bool(true);
        let final_event = transcript.accept(&tail).unwrap();
        assert_eq!(final_event.kind, "final");
        assert_eq!(final_event.text, "The last sentence.");
        assert!(transcript.flush().is_none());
        transcript.accept(&result("unconfirmed only", 2.0, 1.0, false, false));
        assert!(transcript.flush().is_none());
    }
}
