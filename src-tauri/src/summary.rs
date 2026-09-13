use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;

use crate::storage::{
    DefinitionItem, KnowledgePoint, MindMap, SummarySource, TopicSummaryInput, WebInsight,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryRequest {
    pub summary_id: String,
    pub kind: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub course_name: String,
    pub course_context: String,
    pub glossary: Vec<String>,
    pub transcript: String,
    pub provider: String,
    pub workspace_id: String,
    pub model: String,
    pub web_search: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedSummary {
    title: String,
    overview: String,
    points: Vec<String>,
    knowledge_points: Vec<KnowledgePoint>,
    definitions: Vec<DefinitionItem>,
    examples: Vec<String>,
    exam_tips: Vec<String>,
    questions: Vec<String>,
    mind_map: MindMap,
    web_insights: Vec<WebInsight>,
}

#[derive(Clone, Copy, Debug)]
struct SummaryShape {
    points_min: usize,
    points_max: usize,
    knowledge_min: usize,
    knowledge_max: usize,
    definitions_max: usize,
    examples_max: usize,
    exam_tips_max: usize,
    questions_min: usize,
    questions_max: usize,
    branches_min: usize,
    branches_max: usize,
    branch_children_max: usize,
    web_insights_max: usize,
}

fn summary_shape(kind: &str, transcript: &str) -> SummaryShape {
    if kind != "lecture" {
        return SummaryShape {
            points_min: 3,
            points_max: 5,
            knowledge_min: 2,
            knowledge_max: 6,
            definitions_max: 6,
            examples_max: 4,
            exam_tips_max: 4,
            questions_min: 2,
            questions_max: 4,
            branches_min: 2,
            branches_max: 4,
            branch_children_max: 4,
            web_insights_max: 4,
        };
    }

    let substantial_lecture =
        transcript.chars().count() >= 8_000 || transcript.matches("### 阶段 ").count() >= 4;
    SummaryShape {
        points_min: if substantial_lecture { 8 } else { 4 },
        points_max: 16,
        knowledge_min: if substantial_lecture { 10 } else { 4 },
        knowledge_max: 24,
        definitions_max: 20,
        examples_max: 14,
        exam_tips_max: 10,
        questions_min: if substantial_lecture { 6 } else { 3 },
        questions_max: 12,
        branches_min: if substantial_lecture { 5 } else { 3 },
        branches_max: 10,
        branch_children_max: 8,
        web_insights_max: 8,
    }
}

fn summary_schema(shape: SummaryShape) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "title", "overview", "points", "knowledgePoints", "definitions",
            "examples", "examTips", "questions", "mindMap", "webInsights"
        ],
        "properties": {
            "title": { "type": "string" },
            "overview": { "type": "string" },
            "points": {
                "type": "array",
                "minItems": shape.points_min,
                "maxItems": shape.points_max,
                "items": { "type": "string" }
            },
            "knowledgePoints": {
                "type": "array",
                "minItems": shape.knowledge_min,
                "maxItems": shape.knowledge_max,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["title", "explanation", "lecturerEvidence", "importance"],
                    "properties": {
                        "title": { "type": "string" },
                        "explanation": { "type": "string" },
                        "lecturerEvidence": { "type": "string" },
                        "importance": { "type": "string", "enum": ["core", "supporting"] }
                    }
                }
            },
            "definitions": {
                "type": "array",
                "maxItems": shape.definitions_max,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["term", "definition"],
                    "properties": {
                        "term": { "type": "string" },
                        "definition": { "type": "string" }
                    }
                }
            },
            "examples": {
                "type": "array",
                "maxItems": shape.examples_max,
                "items": { "type": "string" }
            },
            "examTips": {
                "type": "array",
                "maxItems": shape.exam_tips_max,
                "items": { "type": "string" }
            },
            "questions": {
                "type": "array",
                "minItems": shape.questions_min,
                "maxItems": shape.questions_max,
                "items": { "type": "string" }
            },
            "mindMap": {
                "type": "object",
                "additionalProperties": false,
                "required": ["root", "branches"],
                "properties": {
                    "root": { "type": "string" },
                    "branches": {
                        "type": "array",
                        "minItems": shape.branches_min,
                        "maxItems": shape.branches_max,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["label", "children"],
                            "properties": {
                                "label": { "type": "string" },
                                "children": {
                                    "type": "array",
                                    "minItems": 1,
                                    "maxItems": shape.branch_children_max,
                                    "items": {
                                        "type": "object",
                                        "additionalProperties": false,
                                        "required": ["label", "note"],
                                        "properties": {
                                            "label": { "type": "string" },
                                            "note": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "webInsights": {
                "type": "array",
                "maxItems": shape.web_insights_max,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": [
                        "title", "summary", "explanation", "keyPoints", "formulas",
                        "howToUse", "sources"
                    ],
                    "properties": {
                        "title": { "type": "string" },
                        "summary": { "type": "string" },
                        "explanation": { "type": "string" },
                        "keyPoints": {
                            "type": "array",
                            "minItems": 2,
                            "maxItems": 6,
                            "items": { "type": "string" }
                        },
                        "formulas": {
                            "type": "array",
                            "maxItems": 4,
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": [
                                    "name", "expression", "variables", "useWhen", "steps",
                                    "workedExample"
                                ],
                                "properties": {
                                    "name": { "type": "string" },
                                    "expression": { "type": "string" },
                                    "variables": {
                                        "type": "array",
                                        "minItems": 1,
                                        "maxItems": 10,
                                        "items": { "type": "string" }
                                    },
                                    "useWhen": { "type": "string" },
                                    "steps": {
                                        "type": "array",
                                        "minItems": 1,
                                        "maxItems": 7,
                                        "items": { "type": "string" }
                                    },
                                    "workedExample": { "type": "string" }
                                }
                            }
                        },
                        "howToUse": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 7,
                            "items": { "type": "string" }
                        },
                        "sources": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 3,
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["title", "url"],
                                "properties": {
                                    "title": { "type": "string" },
                                    "url": { "type": "string" }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

fn output_text(response: &Value) -> Option<&str> {
    response
        .get("output")?
        .as_array()?
        .iter()
        .rev()
        .find(|item| item.get("type").and_then(Value::as_str) == Some("message"))?
        .get("content")?
        .as_array()?
        .iter()
        .rev()
        .find(|content| content.get("type").and_then(Value::as_str) == Some("output_text"))?
        .get("text")?
        .as_str()
}

fn escape_control_characters_in_json_strings(candidate: &str) -> Option<String> {
    let mut repaired = String::with_capacity(candidate.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut changed = false;

    for character in candidate.chars() {
        if !in_string {
            repaired.push(character);
            if character == '"' {
                in_string = true;
            }
            continue;
        }

        if escaped {
            match character {
                '\n' => repaired.push('n'),
                '\r' => repaired.push('r'),
                '\t' => repaired.push('t'),
                '\u{0008}' => repaired.push('b'),
                '\u{000C}' => repaired.push('f'),
                character if character.is_control() => {
                    repaired.push('u');
                    repaired.push_str(&format!("{:04X}", character as u32));
                }
                _ => repaired.push(character),
            }
            changed |= character.is_control();
            escaped = false;
            continue;
        }

        match character {
            '\\' => {
                repaired.push(character);
                escaped = true;
            }
            '"' => {
                repaired.push(character);
                in_string = false;
            }
            '\n' => {
                repaired.push_str("\\n");
                changed = true;
            }
            '\r' => {
                repaired.push_str("\\r");
                changed = true;
            }
            '\t' => {
                repaired.push_str("\\t");
                changed = true;
            }
            '\u{0008}' => {
                repaired.push_str("\\b");
                changed = true;
            }
            '\u{000C}' => {
                repaired.push_str("\\f");
                changed = true;
            }
            character if character.is_control() => {
                repaired.push_str(&format!("\\u{:04X}", character as u32));
                changed = true;
            }
            _ => repaired.push(character),
        }
    }

    changed.then_some(repaired)
}

fn generated_summary_candidate(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.starts_with("```") {
        let without_opening = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```JSON"))
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        without_opening
            .strip_suffix("```")
            .unwrap_or(without_opening)
            .trim()
    } else if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        &trimmed[start..=end]
    } else {
        trimmed
    }
}

fn generated_summary_is_truncated(text: &str) -> bool {
    let candidate = generated_summary_candidate(text);
    match serde_json::from_str::<Value>(candidate) {
        Err(error) if error.is_eof() => true,
        Err(_) => escape_control_characters_in_json_strings(candidate).is_some_and(|repaired| {
            serde_json::from_str::<Value>(&repaired).is_err_and(|error| error.is_eof())
        }),
        Ok(_) => false,
    }
}

fn response_is_incomplete(response: &Value) -> bool {
    response.get("status").and_then(Value::as_str) == Some("incomplete")
        || response
            .get("incomplete_details")
            .is_some_and(|details| !details.is_null())
}

fn compact_summary_retry_payload(payload: &Value, is_lecture: bool) -> Value {
    let mut retry = payload.clone();
    let completion_instruction = "\n前一次结构化输出未完整结束。此次必须优先返回一份完整、可解析的 JSON：缩短每个字段，禁止 Markdown，所有数组和对象必须闭合。此次不要联网拓展，webInsights 必须返回空数组。";
    if let Some(instructions) = retry.get_mut("instructions") {
        let current = instructions.as_str().unwrap_or_default();
        *instructions = Value::String(format!("{current}{completion_instruction}"));
    }
    retry["max_output_tokens"] = Value::from(if is_lecture { 22_000 } else { 7_000 });
    if let Some(object) = retry.as_object_mut() {
        object.remove("tools");
        object.remove("tool_choice");
        object.remove("include");
    }
    retry
}

fn parse_generated_summary(text: &str) -> Result<GeneratedSummary, String> {
    let candidate = generated_summary_candidate(text);

    match serde_json::from_str(candidate) {
        Ok(summary) => Ok(summary),
        Err(original_error) => {
            let Some(repaired) = escape_control_characters_in_json_strings(candidate) else {
                return Err(format!("无法解析结构化总结：{original_error}"));
            };
            serde_json::from_str(&repaired)
                .map_err(|_| format!("无法解析结构化总结：{original_error}"))
        }
    }
}

fn insert_source(
    sources: &mut Vec<SummarySource>,
    seen: &mut HashSet<String>,
    title: Option<&str>,
    url: Option<&str>,
) {
    let Some(url) =
        url.filter(|value| value.starts_with("http://") || value.starts_with("https://"))
    else {
        return;
    };
    if seen.insert(url.to_string()) {
        sources.push(SummarySource {
            title: title
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("参考来源")
                .to_string(),
            url: url.to_string(),
        });
    }
}

fn collect_sources(response: &Value) -> Vec<SummarySource> {
    let mut sources = Vec::new();
    let mut seen = HashSet::new();
    let Some(output) = response.get("output").and_then(Value::as_array) else {
        return sources;
    };

    for item in output {
        if item.get("type").and_then(Value::as_str) == Some("web_search_call") {
            if let Some(items) = item
                .get("action")
                .and_then(|action| action.get("sources"))
                .and_then(Value::as_array)
            {
                for source in items {
                    insert_source(
                        &mut sources,
                        &mut seen,
                        source.get("title").and_then(Value::as_str),
                        source.get("url").and_then(Value::as_str),
                    );
                }
            }
        }

        let Some(content) = item.get("content").and_then(Value::as_array) else {
            continue;
        };
        for part in content {
            let Some(annotations) = part.get("annotations").and_then(Value::as_array) else {
                continue;
            };
            for annotation in annotations {
                if annotation.get("type").and_then(Value::as_str) != Some("url_citation") {
                    continue;
                }
                let citation = annotation.get("url_citation").unwrap_or(annotation);
                insert_source(
                    &mut sources,
                    &mut seen,
                    citation.get("title").and_then(Value::as_str),
                    citation.get("url").and_then(Value::as_str),
                );
            }
        }
    }

    sources.truncate(12);
    sources
}

fn sanitize_insight_sources(insights: &mut [WebInsight]) -> Vec<SummarySource> {
    let mut collected = Vec::new();
    let mut collected_seen = HashSet::new();

    for insight in insights {
        let mut clean = Vec::new();
        let mut seen = HashSet::new();
        for source in &insight.sources {
            insert_source(
                &mut clean,
                &mut seen,
                Some(&source.title),
                Some(&source.url),
            );
        }
        clean.truncate(3);
        for source in &clean {
            insert_source(
                &mut collected,
                &mut collected_seen,
                Some(&source.title),
                Some(&source.url),
            );
        }
        insight.sources = clean;
    }

    collected
}

async fn post_summary_payload(
    provider: &str,
    workspace_id: &str,
    provider_label: &str,
    endpoint: &str,
    api_key: &str,
    payload: &Value,
) -> Result<Value, String> {
    // Whole-lecture responses can legitimately take well over the short timeout
    // used by realtime translation, especially when web search is enabled.
    let summary_client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(240))
        .tcp_keepalive(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let retry_delays_ms = [800];
    let mut retry_count = 0;
    let response = loop {
        let mut request_builder = summary_client
            .post(endpoint)
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .json(payload);
        if provider == "openai" {
            request_builder = request_builder.header(
                "OpenAI-Safety-Identifier",
                "nus-lecture-assistant-local-user",
            );
        }
        match request_builder.send().await {
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
                return Err(super::provider_connection_error(
                    provider_label,
                    "总结",
                    &error,
                    retry_count,
                    provider == "alibaba" && workspace_id.trim().is_empty(),
                ));
            }
        }
    };

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("无法读取总结响应：{error}"))?;
    if !status.is_success() {
        return Err(super::compact_error(provider, &body, status));
    }

    serde_json::from_str(&body).map_err(|error| format!("总结响应格式无效：{error}"))
}

#[tauri::command]
pub async fn generate_topic_summary(request: SummaryRequest) -> Result<TopicSummaryInput, String> {
    if request.transcript.trim().is_empty() {
        return Err("没有可用于总结的课堂内容".to_string());
    }
    if !matches!(request.kind.as_str(), "topic" | "lecture") {
        return Err("不支持的总结类型".to_string());
    }
    if request.summary_id.trim().is_empty() || request.summary_id.chars().count() > 200 {
        return Err("总结标识无效".to_string());
    }
    if request.start_ms < 0 || request.end_ms < request.start_ms {
        return Err("总结时间范围无效".to_string());
    }
    if request.transcript.chars().count() > 200_000 {
        return Err("课堂材料过长，请缩短后再总结".to_string());
    }
    if request.model.trim().is_empty() || request.model.chars().count() > 100 {
        return Err("总结模型名称无效".to_string());
    }

    let mut glossary = String::new();
    for term in request.glossary.iter().take(2_000) {
        let term = term.trim();
        if term.is_empty() {
            continue;
        }
        let separator = if glossary.is_empty() { "" } else { "\n" };
        if glossary.chars().count() + separator.chars().count() + term.chars().count() > 30_000 {
            break;
        }
        glossary.push_str(separator);
        glossary.push_str(term);
    }
    let glossary = if glossary.is_empty() {
        "（无）".to_string()
    } else {
        glossary
    };
    let is_lecture = request.kind == "lecture";
    let shape = summary_shape(&request.kind, &request.transcript);
    let web_instruction = if request.web_search {
        if is_lecture {
            "使用 Web Search 选择 2-6 个最能帮助整课复习的外部知识拓展。每项都必须直接讲授可学习的内容，不能只写“深入了解某主题”或推荐一个学习方向。summary 说明它与本课内容的联系；explanation 用 3-6 句讲清原理、假设、结论及常见误区；keyPoints 给出可以直接复习的实质结论；howToUse 给出实际使用或解题步骤。遇到公式、定律或计算模型时，formulas 必须给出完整可计算表达式、每个变量及单位、适用条件、逐步使用方法，并提供一个带数值代入、结果与解释的算例；例如 Erlang B/C 必须分别给出公式并说明阻塞系统与排队系统的区别。formula.expression 使用标准 LaTeX 表达式，但不要包含 $ 或 $$ 定界符；JSON 中的反斜杠必须正确转义。非计算主题可以让 formulas 为空，但仍须详细解释机制和应用方法。每项 sources 放入 1-3 个本次搜索实际返回的权威链接，优先大学、标准组织、官方文档和可靠教材页面；不能编造 URL。无法找到可核验来源的条目不要输出。联网内容只能放入 webInsights，不得混入课堂知识点或改写教师原意。"
        } else {
            "使用 Web Search 选择 1-3 个与当前小主题高度相关的外部知识拓展。每项必须直接解释可供学生立即学习的具体知识，不能只写主题名称或“建议进一步了解”。summary 说明与刚才课堂内容的联系；explanation、keyPoints 和 howToUse 要讲清原理、结论与使用方法。遇到公式或计算模型时，formulas 必须提供完整可计算表达式、变量及单位、适用条件、步骤和一个带数值代入及结果解释的算例；formula.expression 使用标准 LaTeX 表达式且不包含 $ 或 $$ 定界符，JSON 中的反斜杠必须正确转义。每项 sources 放入 1-3 个本次搜索实际返回的权威链接，不能编造 URL；无法核验来源的条目不要输出。联网内容只能放入 webInsights，不得改写教师原意。"
        }
    } else {
        "不要假装进行了联网检索；webInsights 返回空数组。"
    };
    let material_label = if is_lecture {
        "整课材料（包含按时间排序的阶段总结与原始转写证据）"
    } else {
        "当前课堂语段"
    };
    let input = format!(
        "课程：{}\n课程背景：{}\n术语表：\n{}\n\n{}：\n{}",
        super::clip_characters(request.course_name.trim(), 300),
        super::clip_characters(request.course_context.trim(), 20_000),
        glossary,
        material_label,
        request.transcript.trim()
    );
    let scope_instruction = if is_lecture {
        format!(
            "这是课后使用的整课复习资料，不是高度概括的执行摘要。必须按照教师讲授顺序覆盖整节课的每个主要主题，阶段材料较多时也不能只保留开头和结尾。\n\
             去除阶段之间的重复，但保留定义、机制、因果链、流程、公式及变量含义、成立条件、推导思路、比较关系、教师例子、限制条件和易错点。\n\
             overview 用 4-7 句给出整课路线图；points 输出 {}-{} 条能够串起全课的具体结论。\n\
             knowledgePoints 输出 {}-{} 个可独立复习的知识点。每个 explanation 应用 2-5 句讲清楚“是什么、为什么、如何使用以及适用条件”；lecturerEvidence 写明对应阶段、时间或教师例子，不要只写“教师提到”。\n\
             definitions、examples、examTips 和 questions 应在课堂确有相应内容时尽量完整收录；questions 需要覆盖理解、计算或应用层次。\n\
             mindMap 按课程章节组织，体现主题之间的先后、依赖、对比和应用关系。",
            shape.points_min, shape.points_max, shape.knowledge_min, shape.knowledge_max
        )
    } else {
        format!(
            "这是上课过程中使用的阶段回顾，目标是让刚刚走神或没有听懂的学生在 1-2 分钟内补上当前小主题。只处理当前语段，不回顾整节课，也不要扩展到未提供的课程内容。\n\
             overview 用 2-3 句回答“刚才在讲什么”；points 输出 {}-{} 条按讲授顺序排列的关键结论。\n\
             knowledgePoints 输出 {}-{} 个最需要理解的概念，每个 explanation 用 1-3 句解释概念、因果或步骤，并用 lecturerEvidence 保留教师刚才的例子、公式或说法。\n\
             优先帮助学生理解难点，不要堆砌术语；教师没有讲考试要求时 examTips 必须为空。mindMap 保持紧凑。",
            shape.points_min, shape.points_max, shape.knowledge_min, shape.knowledge_max
        )
    };
    let instructions = format!(
        "你是 NUS 课堂学习助理。请基于输入生成简体中文的结构化课堂总结。\n\
         {scope_instruction}\n\
         必须忠实区分教师讲授内容与外部拓展，不得补造课堂中没有出现的结论、公式、例子或考试要求。\n\
         title 应简短准确；所有字段都要使用完整、可读的句子，避免只有关键词的条目。\n\
         examTips 只记录教师明确提到的考试、作业或易错点；若没有则返回空数组。\n\
         {web_instruction}"
    );

    let (provider_label, _, _) = super::provider_details(&request.provider)?;
    let base_url = super::provider_base_url(&request.provider, &request.workspace_id)?;
    let schema = summary_schema(shape);
    let max_output_tokens = if is_lecture { 18_000 } else { 5_500 };
    let mut payload = if request.provider == "alibaba" {
        json!({
            "model": request.model,
            "instructions": format!(
                "{instructions}\n必须只输出合法 JSON，不要使用 Markdown 代码块。JSON 必须严格符合此 Schema：{}",
                schema
            ),
            "input": input,
            "reasoning": {"effort": "none"},
            "max_output_tokens": max_output_tokens
        })
    } else {
        json!({
            "model": request.model,
            "instructions": instructions,
            "input": input,
            "store": false,
            "reasoning": { "effort": "low" },
            "max_output_tokens": max_output_tokens,
            "text": {
                "format": {
                    "type": "json_schema",
                    "name": "lecture_topic_summary",
                    "strict": true,
                    "schema": schema
                }
            }
        })
    };

    if request.web_search {
        payload["tools"] = json!([{
            "type": "web_search",
            "user_location": {
                "type": "approximate",
                "country": "SG",
                "city": "Singapore",
                "region": "Singapore"
            }
        }]);
        payload["tool_choice"] = json!("auto");
        if request.provider == "openai" {
            payload["include"] = json!(["web_search_call.action.sources"]);
        }
    }

    let api_key = super::read_provider_api_key(&request.provider)?;
    let endpoint = format!("{base_url}/responses");
    let mut response_value = post_summary_payload(
        &request.provider,
        &request.workspace_id,
        provider_label,
        &endpoint,
        &api_key,
        &payload,
    )
    .await?;
    let first_text = output_text(&response_value).map(str::to_string);
    let first_parse = first_text
        .as_deref()
        .ok_or_else(|| "总结服务没有返回文本".to_string())
        .and_then(parse_generated_summary);
    let mut generated = match first_parse {
        Ok(summary) => summary,
        Err(first_error) => {
            let was_truncated = response_is_incomplete(&response_value)
                || first_text
                    .as_deref()
                    .is_some_and(generated_summary_is_truncated);
            let retry_payload = compact_summary_retry_payload(&payload, is_lecture);
            response_value = post_summary_payload(
                &request.provider,
                &request.workspace_id,
                provider_label,
                &endpoint,
                &api_key,
                &retry_payload,
            )
            .await?;
            let retry_text = output_text(&response_value)
                .ok_or_else(|| "总结自动重试后仍没有返回文本".to_string())?;
            parse_generated_summary(retry_text).map_err(|retry_error| {
                let first_reason = if was_truncated {
                    "首次输出被截断"
                } else {
                    "首次输出格式无效"
                };
                format!(
                    "无法解析结构化总结：{first_reason}，自动精简重试后仍失败（{retry_error}；首次错误：{first_error}）"
                )
            })?
        }
    };
    let mut sources = collect_sources(&response_value);
    if !sources.is_empty() {
        let verified_urls = sources
            .iter()
            .map(|source| source.url.as_str())
            .collect::<HashSet<_>>();
        for insight in &mut generated.web_insights {
            insight
                .sources
                .retain(|source| verified_urls.contains(source.url.as_str()));
        }
    }
    let insight_sources = sanitize_insight_sources(&mut generated.web_insights);
    let mut seen = sources
        .iter()
        .map(|source| source.url.clone())
        .collect::<HashSet<_>>();
    for source in insight_sources {
        insert_source(
            &mut sources,
            &mut seen,
            Some(&source.title),
            Some(&source.url),
        );
    }
    sources.truncate(20);

    Ok(TopicSummaryInput {
        id: request.summary_id,
        kind: Some(request.kind),
        start_ms: request.start_ms,
        end_ms: Some(request.end_ms),
        title: generated.title,
        points: generated.points,
        overview: generated.overview,
        knowledge_points: generated.knowledge_points,
        definitions: generated.definitions,
        examples: generated.examples,
        exam_tips: generated.exam_tips,
        questions: generated.questions,
        mind_map: Some(generated.mind_map),
        web_insights: generated.web_insights,
        web_enriched: request.web_search && !sources.is_empty(),
        sources,
        model: Some(request.model),
        is_demo: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_and_deduplicates_sources() {
        let response = json!({
            "output": [
                {
                    "type": "web_search_call",
                    "action": {"sources": [
                        {"title": "NUS", "url": "https://nus.edu.sg/example"},
                        {"title": "Duplicate", "url": "https://nus.edu.sg/example"}
                    ]}
                },
                {
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "{\"title\":\"Topic\"}",
                        "annotations": [{
                            "type": "url_citation",
                            "url": "https://openai.com/research",
                            "title": "Research"
                        }]
                    }]
                }
            ]
        });

        assert_eq!(output_text(&response), Some("{\"title\":\"Topic\"}"));
        let sources = collect_sources(&response);
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].title, "NUS");
        assert_eq!(sources[1].url, "https://openai.com/research");
    }

    #[test]
    fn uses_the_last_message_after_tool_calls() {
        let response = json!({
            "output": [
                {
                    "type": "message",
                    "content": [{"type": "output_text", "text": "{\"points\":["}]
                },
                {"type": "web_search_call", "action": {"sources": []}},
                {
                    "type": "message",
                    "content": [{"type": "output_text", "text": "{\"title\":\"Complete\"}"}]
                }
            ]
        });

        assert_eq!(output_text(&response), Some("{\"title\":\"Complete\"}"));
    }

    #[test]
    fn detects_truncated_json_and_builds_a_compact_retry() {
        let truncated = r#"{"title":"Topic","points":["One","Two""#;
        assert!(generated_summary_is_truncated(truncated));
        assert!(response_is_incomplete(&json!({
            "status": "incomplete",
            "incomplete_details": {"reason": "max_output_tokens"}
        })));

        let payload = json!({
            "instructions": "Return JSON.",
            "max_output_tokens": 5500,
            "tools": [{"type": "web_search"}],
            "tool_choice": "auto",
            "include": ["web_search_call.action.sources"]
        });
        let retry = compact_summary_retry_payload(&payload, false);
        assert_eq!(retry["max_output_tokens"], 7_000);
        assert!(retry.get("tools").is_none());
        assert!(retry.get("tool_choice").is_none());
        assert!(retry.get("include").is_none());
        assert!(retry["instructions"]
            .as_str()
            .is_some_and(|value| value.contains("webInsights 必须返回空数组")));
    }

    #[test]
    fn parses_json_from_compatible_provider_code_fence() {
        let body = json!({
            "title": "Elasticity",
            "overview": "Overview",
            "points": ["One", "Two", "Three"],
            "knowledgePoints": [],
            "definitions": [],
            "examples": [],
            "examTips": [],
            "questions": ["Q1", "Q2"],
            "mindMap": {"root": "Elasticity", "branches": []},
            "webInsights": []
        });
        let fenced = format!("```json\n{body}\n```");
        let parsed = parse_generated_summary(&fenced).expect("valid summary");
        assert_eq!(parsed.title, "Elasticity");
    }

    #[test]
    fn parses_detailed_web_insight_and_keeps_old_fields_compatible() {
        let body = json!({
            "title": "Traffic engineering",
            "overview": "Overview",
            "points": ["One", "Two", "Three"],
            "knowledgePoints": [],
            "definitions": [],
            "examples": [],
            "examTips": [],
            "questions": ["Q1", "Q2"],
            "mindMap": {"root": "Traffic", "branches": []},
            "webInsights": [{
                "title": "Erlang B",
                "summary": "Used for lost-call systems.",
                "explanation": "The model estimates blocking when calls cannot queue.",
                "keyPoints": ["Blocked calls leave", "Holding times are independent"],
                "formulas": [{
                    "name": "Erlang B",
                    "expression": "B(A,N) = (A^N/N!) / sum(k=0..N)(A^k/k!)",
                    "variables": ["A: offered traffic in Erlangs", "N: number of channels"],
                    "useWhen": "Use when blocked calls are cleared.",
                    "steps": ["Calculate A", "Choose N", "Evaluate B"],
                    "workedExample": "For A=2 and N=3, B is about 0.21."
                }],
                "howToUse": ["Set a blocking target", "Find the smallest N that meets it"],
                "sources": [{"title": "University notes", "url": "https://example.edu/erlang"}]
            }, {
                "title": "Legacy item",
                "summary": "An item saved by an older app version."
            }]
        });

        let parsed = parse_generated_summary(&body.to_string()).expect("valid detailed summary");
        assert_eq!(parsed.web_insights[0].formulas.len(), 1);
        assert_eq!(parsed.web_insights[0].sources[0].title, "University notes");
        assert!(parsed.web_insights[1].explanation.is_empty());
        assert!(parsed.web_insights[1].formulas.is_empty());
    }

    #[test]
    fn filters_invalid_and_duplicate_insight_sources() {
        let mut insights = vec![WebInsight {
            title: "Erlang".to_string(),
            sources: vec![
                SummarySource {
                    title: "Valid".to_string(),
                    url: "https://example.edu/erlang".to_string(),
                },
                SummarySource {
                    title: "Duplicate".to_string(),
                    url: "https://example.edu/erlang".to_string(),
                },
                SummarySource {
                    title: "Invalid".to_string(),
                    url: "javascript:alert(1)".to_string(),
                },
            ],
            ..WebInsight::default()
        }];

        let collected = sanitize_insight_sources(&mut insights);
        assert_eq!(insights[0].sources.len(), 1);
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].url, "https://example.edu/erlang");
    }

    #[test]
    fn repairs_unescaped_control_characters_inside_json_strings() {
        let body = json!({
            "title": "Elasticity",
            "overview": "Overview",
            "points": ["One", "Two", "Three"],
            "knowledgePoints": [],
            "definitions": [],
            "examples": [],
            "examTips": [],
            "questions": ["Q1", "Q2"],
            "mindMap": {"root": "Elasticity", "branches": []},
            "webInsights": []
        });
        let malformed = body
            .to_string()
            .replace("Overview", "First line\nSecond\tline\0");

        let parsed = parse_generated_summary(&malformed).expect("repaired summary");
        assert_eq!(parsed.overview, "First line\nSecond\tline\0");
    }

    #[test]
    fn gives_whole_lecture_summaries_a_much_larger_structure() {
        let topic = summary_shape("topic", "short topic");
        let lecture = summary_shape("lecture", &"evidence ".repeat(1_000));

        assert_eq!(topic.points_max, 5);
        assert_eq!(topic.knowledge_max, 6);
        assert_eq!(lecture.points_min, 8);
        assert_eq!(lecture.points_max, 16);
        assert_eq!(lecture.knowledge_min, 10);
        assert_eq!(lecture.knowledge_max, 24);
        assert!(lecture.examples_max > topic.examples_max);

        let schema = summary_schema(lecture);
        assert_eq!(
            schema
                .pointer("/properties/knowledgePoints/maxItems")
                .and_then(Value::as_u64),
            Some(24)
        );
        assert_eq!(
            schema
                .pointer("/properties/webInsights/items/properties/formulas/maxItems")
                .and_then(Value::as_u64),
            Some(4)
        );
        assert_eq!(
            schema
                .pointer("/properties/webInsights/items/properties/sources/minItems")
                .and_then(Value::as_u64),
            Some(1)
        );
    }

    #[test]
    fn keeps_short_whole_lecture_summaries_proportional() {
        let lecture = summary_shape("lecture", "a short stopped session");
        assert_eq!(lecture.points_min, 4);
        assert_eq!(lecture.knowledge_min, 4);
        assert_eq!(lecture.questions_min, 3);
    }
}
