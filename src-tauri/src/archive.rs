use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::{Connection, OpenFlags};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

use crate::storage::Database;

fn safe_name(value: &str) -> String {
    let clean = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    clean.trim().chars().take(80).collect()
}

fn append_string_list(markdown: &mut String, title: &str, values: Option<&serde_json::Value>) {
    let Some(values) = values.and_then(serde_json::Value::as_array) else {
        return;
    };
    let values = values
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return;
    }
    markdown.push_str(&format!("#### {title}\n\n"));
    for value in values {
        markdown.push_str(&format!("- {value}\n"));
    }
    markdown.push('\n');
}

fn append_nested_string_list(
    markdown: &mut String,
    title: &str,
    values: Option<&serde_json::Value>,
) {
    let Some(values) = values.and_then(serde_json::Value::as_array) else {
        return;
    };
    let values = values
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return;
    }
    markdown.push_str(&format!("###### {title}\n\n"));
    for value in values {
        markdown.push_str(&format!("- {value}\n"));
    }
    markdown.push('\n');
}

fn append_summary_markdown(markdown: &mut String, title: &str, value: &serde_json::Value) {
    let is_lecture = value.get("kind").and_then(serde_json::Value::as_str) == Some("lecture");
    let label = if is_lecture {
        "整课复习"
    } else {
        "阶段回顾"
    };
    markdown.push_str(&format!("### {label}：{title}\n\n"));
    if let Some(overview) = value
        .get("overview")
        .and_then(serde_json::Value::as_str)
        .filter(|overview| !overview.trim().is_empty())
    {
        markdown.push_str(&format!("{overview}\n\n"));
    }
    append_string_list(
        markdown,
        if is_lecture {
            "整课主线"
        } else {
            "本段重点"
        },
        value.get("points"),
    );

    if let Some(points) = value
        .get("knowledgePoints")
        .and_then(serde_json::Value::as_array)
    {
        if !points.is_empty() {
            markdown.push_str(&format!(
                "#### {}\n\n",
                if is_lecture {
                    "完整知识体系"
                } else {
                    "重点解释"
                }
            ));
            for point in points {
                let point_title = point
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("知识点");
                let importance = match point.get("importance").and_then(serde_json::Value::as_str) {
                    Some("core") => "（核心）",
                    _ => "",
                };
                markdown.push_str(&format!("##### {point_title}{importance}\n\n"));
                if let Some(explanation) =
                    point.get("explanation").and_then(serde_json::Value::as_str)
                {
                    markdown.push_str(&format!("{explanation}\n\n"));
                }
                if let Some(evidence) = point
                    .get("lecturerEvidence")
                    .and_then(serde_json::Value::as_str)
                    .filter(|evidence| !evidence.trim().is_empty())
                {
                    markdown.push_str(&format!("> 课堂依据：{evidence}\n\n"));
                }
            }
        }
    }

    if let Some(definitions) = value
        .get("definitions")
        .and_then(serde_json::Value::as_array)
    {
        if !definitions.is_empty() {
            markdown.push_str("#### 定义与术语\n\n");
            for definition in definitions {
                let term = definition
                    .get("term")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("术语");
                let detail = definition
                    .get("definition")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                markdown.push_str(&format!("- **{term}**：{detail}\n"));
            }
            markdown.push('\n');
        }
    }

    append_string_list(markdown, "教师例子与类比", value.get("examples"));
    append_string_list(markdown, "考试、作业与易错提示", value.get("examTips"));
    append_string_list(markdown, "复习问题", value.get("questions"));

    if let Some(branches) = value
        .pointer("/mindMap/branches")
        .and_then(serde_json::Value::as_array)
    {
        if !branches.is_empty() {
            markdown.push_str("#### 思维导图\n\n");
            for branch in branches {
                let branch_title = branch
                    .get("label")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("主题");
                markdown.push_str(&format!("- **{branch_title}**\n"));
                if let Some(children) = branch.get("children").and_then(serde_json::Value::as_array)
                {
                    for child in children {
                        let child_title = child
                            .get("label")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("知识点");
                        let note = child
                            .get("note")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default();
                        markdown.push_str(&format!("  - {child_title}：{note}\n"));
                    }
                }
            }
            markdown.push('\n');
        }
    }

    if let Some(insights) = value
        .get("webInsights")
        .and_then(serde_json::Value::as_array)
        .filter(|insights| !insights.is_empty())
    {
        markdown.push_str("#### 相关知识拓展\n\n");
        for insight in insights {
            let insight_title = insight
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("拓展知识");
            markdown.push_str(&format!("##### {insight_title}\n\n"));

            for field in ["summary", "explanation"] {
                if let Some(content) = insight
                    .get(field)
                    .and_then(serde_json::Value::as_str)
                    .filter(|content| !content.trim().is_empty())
                {
                    markdown.push_str(&format!("{content}\n\n"));
                }
            }

            append_nested_string_list(markdown, "关键结论", insight.get("keyPoints"));

            if let Some(formulas) = insight
                .get("formulas")
                .and_then(serde_json::Value::as_array)
                .filter(|formulas| !formulas.is_empty())
            {
                markdown.push_str("###### 公式与计算\n\n");
                for formula in formulas {
                    let name = formula
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("公式");
                    markdown.push_str(&format!("**{name}**\n\n"));
                    if let Some(expression) = formula
                        .get("expression")
                        .and_then(serde_json::Value::as_str)
                        .filter(|expression| !expression.trim().is_empty())
                    {
                        let expression = expression.trim();
                        if expression.contains('\\') || expression.starts_with('$') {
                            let expression = expression
                                .strip_prefix("$$")
                                .and_then(|value| value.strip_suffix("$$"))
                                .or_else(|| {
                                    expression
                                        .strip_prefix('$')
                                        .and_then(|value| value.strip_suffix('$'))
                                })
                                .unwrap_or(expression)
                                .trim();
                            markdown.push_str(&format!("$$\n{expression}\n$$\n\n"));
                        } else {
                            markdown.push_str(&format!("`{expression}`\n\n"));
                        }
                    }
                    append_nested_string_list(markdown, "变量", formula.get("variables"));
                    if let Some(use_when) = formula
                        .get("useWhen")
                        .and_then(serde_json::Value::as_str)
                        .filter(|use_when| !use_when.trim().is_empty())
                    {
                        markdown.push_str(&format!("**适用场景**：{use_when}\n\n"));
                    }
                    append_nested_string_list(markdown, "使用步骤", formula.get("steps"));
                    if let Some(example) = formula
                        .get("workedExample")
                        .and_then(serde_json::Value::as_str)
                        .filter(|example| !example.trim().is_empty())
                    {
                        markdown.push_str(&format!("**算例**：{example}\n\n"));
                    }
                }
            }

            append_nested_string_list(markdown, "如何应用", insight.get("howToUse"));
            if let Some(sources) = insight
                .get("sources")
                .and_then(serde_json::Value::as_array)
                .filter(|sources| !sources.is_empty())
            {
                markdown.push_str("###### 进一步阅读\n\n");
                for source in sources {
                    let source_title = source
                        .get("title")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("参考来源");
                    if let Some(url) = source
                        .get("url")
                        .and_then(serde_json::Value::as_str)
                        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
                    {
                        markdown.push_str(&format!("- [{source_title}]({url})\n"));
                    }
                }
                markdown.push('\n');
            }
        }
    }

    if let Some(sources) = value
        .get("sources")
        .and_then(serde_json::Value::as_array)
        .filter(|sources| !sources.is_empty())
    {
        markdown.push_str("#### 参考来源\n\n");
        for source in sources {
            let source_title = source
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("参考来源");
            if let Some(url) = source
                .get("url")
                .and_then(serde_json::Value::as_str)
                .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            {
                markdown.push_str(&format!("- [{source_title}]({url})\n"));
            }
        }
        markdown.push('\n');
    }
}

fn timestamp_label() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "backup".to_string())
}

fn add_directory_to_zip(
    writer: &mut ZipWriter<File>,
    root: &Path,
    directory: &Path,
    options: SimpleFileOptions,
) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
        let canonical_path = path.canonicalize().map_err(|error| error.to_string())?;
        if !canonical_path.starts_with(&canonical_root) {
            return Err("备份文件路径超出应用数据目录".to_string());
        }
        if file_type.is_dir() {
            add_directory_to_zip(writer, root, &path, options)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        writer
            .start_file(relative, options)
            .map_err(|error| error.to_string())?;
        let mut file = File::open(&path).map_err(|error| error.to_string())?;
        std::io::copy(&mut file, writer).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn create_app_backup(app: AppHandle, database: State<'_, Database>) -> Result<String, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let backup_dir = data_dir.join("backups");
    fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    let snapshot = backup_dir.join(format!("snapshot-{}.sqlite3", Uuid::new_v4()));
    {
        let connection = database
            .0
            .lock()
            .map_err(|_| "课堂数据库锁已损坏".to_string())?;
        connection
            .execute("VACUUM INTO ?1", [snapshot.to_string_lossy().as_ref()])
            .map_err(|error| format!("无法创建数据库快照：{error}"))?;
    }
    let archive_path = backup_dir.join(format!("NUS课堂同传-{}.nuslecture", timestamp_label()));
    let file = File::create(&archive_path).map_err(|error| error.to_string())?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file("lecture-assistant.sqlite3", options)
        .map_err(|error| error.to_string())?;
    let mut snapshot_file = File::open(&snapshot).map_err(|error| error.to_string())?;
    std::io::copy(&mut snapshot_file, &mut writer).map_err(|error| error.to_string())?;
    add_directory_to_zip(&mut writer, &data_dir, &data_dir.join("documents"), options)?;
    writer.finish().map_err(|error| error.to_string())?;
    let _ = fs::remove_file(snapshot);
    Ok(archive_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn stage_app_restore(app: AppHandle, data_base64: String) -> Result<String, String> {
    let bytes = BASE64
        .decode(data_base64.as_bytes())
        .map_err(|_| "备份文件内容无效".to_string())?;
    if bytes.is_empty() || bytes.len() > 500 * 1024 * 1024 {
        return Err("备份文件必须小于 500MB".to_string());
    }
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    stage_restore_bytes(&data_dir, bytes)?;
    Ok("备份已验证，将在下次启动应用时恢复".to_string())
}

fn stage_restore_bytes(data_dir: &Path, bytes: Vec<u8>) -> Result<(), String> {
    let pending = data_dir.join("restore-pending");
    let staging = data_dir.join(format!("restore-staging-{}", Uuid::new_v4()));
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let result = (|| -> Result<(), String> {
        let mut archive = ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|error| format!("无法打开备份文件：{error}"))?;
        if archive.len() > 20_000 {
            return Err("备份中的文件数量异常，已停止恢复".to_string());
        }
        let mut total_extracted = 0u64;
        for index in 0..archive.len() {
            let file = archive.by_index(index).map_err(|error| error.to_string())?;
            let relative = file
                .enclosed_name()
                .ok_or_else(|| "备份包含不安全的文件路径".to_string())?;
            if relative != PathBuf::from("lecture-assistant.sqlite3")
                && !relative.starts_with("documents")
            {
                continue;
            }
            let destination = staging.join(&relative);
            if file.is_dir() {
                fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
                continue;
            }
            let entry_limit = if relative == PathBuf::from("lecture-assistant.sqlite3") {
                1024 * 1024 * 1024
            } else {
                64 * 1024 * 1024
            };
            if file.size() > entry_limit {
                return Err("备份中的单个文件异常大，已停止恢复".to_string());
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut output = File::create(destination).map_err(|error| error.to_string())?;
            let copied = std::io::copy(&mut file.take(entry_limit + 1), &mut output)
                .map_err(|error| error.to_string())?;
            if copied > entry_limit {
                return Err("备份中的单个文件超过安全大小，已停止恢复".to_string());
            }
            total_extracted = total_extracted.saturating_add(copied);
            if total_extracted > 2 * 1024 * 1024 * 1024 {
                return Err("备份解压后的内容超过 2GB，已停止恢复".to_string());
            }
        }
        let database_path = staging.join("lecture-assistant.sqlite3");
        if !database_path.exists() {
            return Err("备份中没有课堂数据库".to_string());
        }
        let mut check =
            Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
                .map_err(|error| error.to_string())?;
        let quick_check: String = check
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if quick_check != "ok" {
            return Err(format!("备份数据库校验失败：{quick_check}"));
        }
        let table_count: i64 = check
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('courses','lectures','transcript_segments')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if table_count != 3 {
            return Err("备份数据库结构不完整".to_string());
        }
        let document_paths = {
            let mut statement = check
                .prepare("SELECT id, course_id, lecture_id, stored_path FROM course_documents")
                .map_err(|error| error.to_string())?;
            let paths = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            drop(statement);
            paths
        };
        let transaction = check.transaction().map_err(|error| error.to_string())?;
        for (document_id, course_id, lecture_id, old_path) in document_paths {
            let file_name = old_path
                .rsplit(['/', '\\'])
                .next()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "备份中的 Slides 路径无效".to_string())?;
            let mut relative = PathBuf::from("documents").join(course_id.to_string());
            if let Some(lecture_id) = lecture_id {
                relative = relative.join(lecture_id.to_string());
            }
            relative = relative.join(file_name);
            if !staging.join(&relative).is_file() {
                return Err(format!("备份缺少 Slides 原文件：{file_name}"));
            }
            let restored_path = data_dir.join(relative).to_string_lossy().to_string();
            transaction
                .execute(
                    "UPDATE course_documents SET stored_path = ?1 WHERE id = ?2",
                    rusqlite::params![restored_path, document_id],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if pending.exists() {
        fs::remove_dir_all(&pending).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&staging, &pending) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error.to_string());
    }
    Ok(())
}

pub fn apply_pending_restore(data_dir: &Path) -> Result<(), String> {
    let pending = data_dir.join("restore-pending");
    let pending_database = pending.join("lecture-assistant.sqlite3");
    if !pending_database.exists() {
        return Ok(());
    }
    let current_database = data_dir.join("lecture-assistant.sqlite3");
    let restore_id = Uuid::new_v4();
    let next_database = data_dir.join(format!("lecture-assistant.restore-{restore_id}.sqlite3"));
    let previous_database =
        data_dir.join(format!("lecture-assistant.previous-{restore_id}.sqlite3"));
    let current_documents = data_dir.join("documents");
    let next_documents = data_dir.join(format!("documents-restore-{restore_id}"));
    let previous_documents = data_dir.join(format!("documents-previous-{restore_id}"));
    if current_database.exists() {
        let backup_dir = data_dir.join("backups");
        fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;
        fs::copy(
            &current_database,
            backup_dir.join(format!("pre-restore-{}.sqlite3", timestamp_label())),
        )
        .map_err(|error| error.to_string())?;
    }
    fs::copy(&pending_database, &next_database).map_err(|error| error.to_string())?;
    let pending_documents = pending.join("documents");
    if pending_documents.exists() {
        if let Err(error) = copy_directory(&pending_documents, &next_documents) {
            let _ = fs::remove_file(&next_database);
            let _ = fs::remove_dir_all(&next_documents);
            return Err(error);
        }
    } else {
        fs::create_dir_all(&next_documents).map_err(|error| error.to_string())?;
    }
    let _ = fs::remove_file(data_dir.join("lecture-assistant.sqlite3-wal"));
    let _ = fs::remove_file(data_dir.join("lecture-assistant.sqlite3-shm"));
    if current_documents.exists() {
        fs::rename(&current_documents, &previous_documents).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&next_documents, &current_documents) {
        if previous_documents.exists() {
            let _ = fs::rename(&previous_documents, &current_documents);
        }
        let _ = fs::remove_file(&next_database);
        return Err(error.to_string());
    }
    if current_database.exists() {
        if let Err(error) = fs::rename(&current_database, &previous_database) {
            let _ = fs::remove_dir_all(&current_documents);
            if previous_documents.exists() {
                let _ = fs::rename(&previous_documents, &current_documents);
            }
            let _ = fs::remove_file(&next_database);
            return Err(error.to_string());
        }
    }
    if let Err(error) = fs::rename(&next_database, &current_database) {
        if previous_database.exists() {
            let _ = fs::rename(&previous_database, &current_database);
        }
        let _ = fs::remove_dir_all(&current_documents);
        if previous_documents.exists() {
            let _ = fs::rename(&previous_documents, &current_documents);
        }
        return Err(error.to_string());
    }
    let _ = fs::remove_file(previous_database);
    let _ = fs::remove_dir_all(previous_documents);
    fs::remove_dir_all(pending).map_err(|error| error.to_string())?;
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn export_lecture_markdown(
    app: AppHandle,
    database: State<'_, Database>,
    lecture_id: i64,
) -> Result<String, String> {
    let connection = database
        .0
        .lock()
        .map_err(|_| "课堂数据库锁已损坏".to_string())?;
    let (title, course_name): (String, String) = connection
        .query_row(
            "SELECT l.title, c.name FROM lectures l JOIN courses c ON c.id = l.course_id WHERE l.id = ?1",
            [lecture_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    let mut markdown = format!("# {title}\n\n课程：{course_name}\n\n## 课堂记录\n\n");
    let mut transcript = connection
        .prepare(
            "SELECT start_ms, english, chinese FROM transcript_segments WHERE lecture_id = ?1 ORDER BY seq",
        )
        .map_err(|error| error.to_string())?;
    for row in transcript
        .query_map([lecture_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?
    {
        let (start_ms, english, chinese) = row.map_err(|error| error.to_string())?;
        markdown.push_str(&format!(
            "### {:02}:{:02}\n\n{}\n\n{}\n\n",
            start_ms / 60_000,
            start_ms / 1_000 % 60,
            english,
            chinese
        ));
    }
    markdown.push_str("## 课堂总结与复习资料\n\n");
    let mut summaries = connection
        .prepare("SELECT title, content_json FROM topic_summaries WHERE lecture_id = ?1 ORDER BY start_ms")
        .map_err(|error| error.to_string())?;
    for row in summaries
        .query_map([lecture_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
    {
        let (summary_title, content) = row.map_err(|error| error.to_string())?;
        let value: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        append_summary_markdown(&mut markdown, &summary_title, &value);
    }
    drop(summaries);
    drop(transcript);
    drop(connection);
    let export_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("exports");
    fs::create_dir_all(&export_dir).map_err(|error| error.to_string())?;
    let export_title = safe_name(&title);
    let export_title = if export_title.is_empty() {
        "课堂记录".to_string()
    } else {
        export_title
    };
    let path = export_dir.join(format!("{export_title}-{lecture_id}.md"));
    let mut file = File::create(&path).map_err(|error| error.to_string())?;
    file.write_all(markdown.as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("nus-lecture-{label}-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("test directory");
        path
    }

    fn restore_archive(database: &[u8], files: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer
            .start_file("lecture-assistant.sqlite3", options)
            .expect("database entry");
        writer.write_all(database).expect("database bytes");
        for (path, bytes) in files {
            writer.start_file(*path, options).expect("file entry");
            writer.write_all(bytes).expect("file bytes");
        }
        writer.finish().expect("archive").into_inner()
    }

    #[test]
    fn makes_export_titles_safe_for_all_desktop_platforms() {
        assert_eq!(safe_name("Week 1: AI/ML?"), "Week 1_ AI_ML_");
    }

    #[test]
    fn restore_is_staged_atomically_and_rewrites_document_paths() {
        let data_dir = test_directory("restore");
        let source_database = data_dir.join("source.sqlite3");
        let connection = Connection::open(&source_database).expect("source database");
        connection
            .execute_batch(
                "CREATE TABLE courses (id INTEGER PRIMARY KEY);
                 CREATE TABLE lectures (id INTEGER PRIMARY KEY);
                 CREATE TABLE transcript_segments (id TEXT);
                 CREATE TABLE course_documents (
                    id INTEGER PRIMARY KEY,
                    course_id INTEGER NOT NULL,
                    lecture_id INTEGER,
                    stored_path TEXT NOT NULL
                 );
                 INSERT INTO courses VALUES (1);
                 INSERT INTO lectures VALUES (2);
                 INSERT INTO course_documents
                 VALUES (3, 1, 2, 'C:\\old-app\\documents\\1\\2\\slide.pdf');",
            )
            .expect("schema");
        drop(connection);
        let database_bytes = fs::read(&source_database).expect("database bytes");
        let archive = restore_archive(
            &database_bytes,
            &[("documents/1/2/slide.pdf", b"slide contents")],
        );

        stage_restore_bytes(&data_dir, archive).expect("valid restore");

        let staged_database = data_dir.join("restore-pending/lecture-assistant.sqlite3");
        let connection = Connection::open(staged_database).expect("staged database");
        let restored_path: String = connection
            .query_row(
                "SELECT stored_path FROM course_documents WHERE id = 3",
                [],
                |row| row.get(0),
            )
            .expect("restored path");
        assert_eq!(
            PathBuf::from(restored_path),
            data_dir.join("documents/1/2/slide.pdf")
        );
        assert!(data_dir
            .join("restore-pending/documents/1/2/slide.pdf")
            .is_file());
        fs::remove_dir_all(data_dir).expect("cleanup");
    }

    #[test]
    fn failed_restore_does_not_replace_an_existing_pending_backup() {
        let data_dir = test_directory("failed-restore");
        let pending = data_dir.join("restore-pending");
        fs::create_dir_all(&pending).expect("pending directory");
        fs::write(pending.join("marker"), b"keep me").expect("marker");
        let invalid = restore_archive(b"not a sqlite database", &[]);

        assert!(stage_restore_bytes(&data_dir, invalid).is_err());
        assert_eq!(
            fs::read(pending.join("marker")).expect("marker remains"),
            b"keep me"
        );
        assert!(fs::read_dir(&data_dir)
            .expect("data directory")
            .all(|entry| !entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .starts_with("restore-staging-")));
        fs::remove_dir_all(data_dir).expect("cleanup");
    }

    #[test]
    fn exports_detailed_whole_lecture_revision_content() {
        let summary = serde_json::json!({
            "kind": "lecture",
            "overview": "全课路线图",
            "points": ["主线一"],
            "knowledgePoints": [{
                "title": "话务强度",
                "explanation": "呼叫率乘以平均保持时间。",
                "lecturerEvidence": "教师计算了 1/30 Erlang。",
                "importance": "core"
            }],
            "definitions": [{"term": "Erlang", "definition": "平均占用程度"}],
            "examples": ["每小时一次两分钟通话"],
            "examTips": ["统一时间单位"],
            "questions": ["如何计算话务强度？"],
            "mindMap": {"branches": [{
                "label": "流量模型",
                "children": [{"label": "Erlang", "note": "衡量信道占用"}]
            }]},
            "webInsights": [{
                "title": "Erlang B 阻塞概率",
                "summary": "用于无排队的损失系统。",
                "explanation": "该模型计算全部信道繁忙时新呼叫被阻塞的概率。",
                "keyPoints": ["阻塞呼叫会离开系统"],
                "formulas": [{
                    "name": "Erlang B",
                    "expression": "B(A,N) = (A^N/N!) / sum(k=0..N)(A^k/k!)",
                    "variables": ["A：话务量（Erlang）", "N：信道数"],
                    "useWhen": "呼叫无法等待时使用。",
                    "steps": ["计算 A", "代入 N"],
                    "workedExample": "A=2、N=3 时计算阻塞概率。"
                }],
                "howToUse": ["先确定可接受的阻塞率"],
                "sources": [{"title": "课程资料", "url": "https://example.edu/erlang"}]
            }],
            "sources": [{"title": "完整参考", "url": "https://example.edu/reference"}]
        });
        let mut markdown = String::new();
        append_summary_markdown(&mut markdown, "容量规划", &summary);

        assert!(markdown.contains("### 整课复习：容量规划"));
        assert!(markdown.contains("#### 完整知识体系"));
        assert!(markdown.contains("课堂依据：教师计算了 1/30 Erlang"));
        assert!(markdown.contains("**Erlang**：平均占用程度"));
        assert!(markdown.contains("#### 思维导图"));
        assert!(markdown.contains("#### 相关知识拓展"));
        assert!(markdown.contains("B(A,N) = (A^N/N!)"));
        assert!(markdown.contains("**算例**：A=2、N=3"));
        assert!(markdown.contains("[课程资料](https://example.edu/erlang)"));
        assert!(markdown.contains("[完整参考](https://example.edu/reference)"));
    }
}
