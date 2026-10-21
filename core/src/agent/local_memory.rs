//! Cross-task recall for local socai runs.
//!
//! A completed agent run asks that run's model for a task note, appends one
//! index line, and refreshes two files. `memory_tasks.md` keeps one note per
//! task. `memory_summary.md` is the text injected into the next run: older
//! history is one compressed narrative, and the newest tasks that have not
//! yet been folded in are copied in verbatim.
//!
//! Folding is stepped. While fewer than five notes are waiting, they stay in
//! the summary as written. The fifth note triggers one model call that
//! rewrites the previous narrative plus those five notes into a single
//! summary of at most 1000 characters. The next notes then accumulate again.
//!
//! This is intentionally smaller than Codex memories: no secret redaction,
//! no cross-process lock, and no separate note when the user asks to forget
//! something. A lost update between two runs finishing at the same moment is
//! acceptable; the next completed run rebuilds both files from the index.

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::file_bash_tools::socai_home_dir;

const MAX_STORED_ENTRIES: usize = 64;
const TASK_MAX_CHARS: usize = 240;
const NOTE_MAX_CHARS: usize = 220;
const ROLLUP_MAX_CHARS: usize = 1_000;
const FOLD_BATCH: usize = 5;
const REPORT_HEAD_CHARS: usize = 5_000;
const REPORT_TAIL_CHARS: usize = 2_500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    run_id: String,
    #[serde(default)]
    run_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    task: String,
    finished_at_ms: u64,
    note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryState {
    #[serde(default)]
    compressed: String,
    #[serde(default)]
    folded_run_ids: Vec<String>,
}

pub fn memories_dir() -> PathBuf {
    socai_home_dir().join("memories")
}

/// Developer-instruction block for the next run. Empty when no summary exists.
pub fn memory_prompt() -> String {
    let summary = fs::read_to_string(memories_dir().join("memory_summary.md")).unwrap_or_default();
    let summary = summary.trim();
    if summary.is_empty() {
        return String::new();
    }
    format!(
        "## Local memory\n\n\
Earlier completed tasks on this machine are summarized below, including what \
the user has been trying to find out and what was found. Use them when the \
request depends on prior research, coverage, decisions, or what this user \
cares about. Skip them for a self-contained question. The current \
conversation is already in the message history.\n\n\
The summary is recall, not fresh evidence, and it does not include file paths. \
Per-task notes are in `{tasks}`. When a point would change the answer, open \
`{index}`, match the task, and read that entry's run directory, starting with \
`report.md`. Do not edit the memories directory.\n\n\
{summary}",
        tasks = memories_dir().join("memory_tasks.md").display(),
        index = memories_dir().join("entries.jsonl").display()
    )
}

/// Instructions for the model that condenses one finished report.
pub(crate) fn summary_prompt(task: &str, report: &str) -> (String, String) {
    let system = "\
你在给本地研究助手写一条跨任务记忆，用来积累对这个用户的了解。只输出记忆本身，不要前言。\
三到四句，总共不超过180个字。先写用户这次关心的问题：在问什么、想判断什么、在意什么。\
再写得到的结果。不要只剩结论。\
不要写后续、下一步、路径、文件名、命令或客套话。";
    let user = format!(
        "任务：\n{}\n\n报告：\n{}",
        collapse_ws(task),
        bounded_report(report)
    );
    (system.to_string(), user)
}

/// Instructions for folding the previous summary and one batch of task notes.
pub(crate) fn rollup_prompt(previous: &str, batch: &str) -> (String, String) {
    let system = "\
你在维护一份对这个用户的总记忆。把已有总记忆和这批新任务收成一篇新的总记忆。\
只输出正文，不要标题或前言。控制在900个字以内，必须在句号处结束。\
每个主题都先留下用户当时的问题，再留下对应结果。\
同一主题多次出现可以合并，不同主题的关切不要合成一句。\
字数不够时先删结果里的数字细节，不要删问题。\
不要写后续计划、下一步、路径、文件名或客套话。";
    let previous = if previous.trim().is_empty() {
        "无".to_string()
    } else {
        previous.trim().to_string()
    };
    let user = format!("已有总记忆：\n{previous}\n\n新的任务：\n{batch}");
    (system.to_string(), user)
}

/// Ask for a shorter complete summary when a rollup ran past the budget.
pub(crate) fn shorten_prompt(text: &str) -> (String, String) {
    let system = "\
下面这篇总记忆超长了。压到900个字以内，只输出正文，不要标题。\
每个主题只留一句：先是用户当时的问题，再是结果。句子必须完整。\
不同主题不要合成一句，后面的主题不能删。\
不要写后续计划、下一步、路径或文件名。";
    (system.to_string(), text.trim().to_string())
}

/// Keep a model note only when it is a short recall line.
pub(crate) fn accept_summary_note(text: &str) -> anyhow::Result<String> {
    let note = collapse_ws(text);
    let note =
        note.trim_matches(|ch| ch == '"' || ch == '「' || ch == '」' || ch == '“' || ch == '”');
    let note = note.trim();
    if note.is_empty() {
        anyhow::bail!("memory note was empty");
    }
    Ok(truncate_chars(note, NOTE_MAX_CHARS))
}

/// Keep a rolled-up history inside the character budget, ending on a sentence.
pub(crate) fn accept_rollup(text: &str) -> anyhow::Result<String> {
    let text = text.trim();
    let text = text.trim_matches(|ch| ch == '"' || ch == '“' || ch == '”');
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("memory rollup was empty");
    }
    if text.chars().count() <= ROLLUP_MAX_CHARS {
        return Ok(text.to_string());
    }
    let end = text
        .char_indices()
        .nth(ROLLUP_MAX_CHARS)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    let sliced = &text[..end];
    if let Some(pos) = sliced.rfind(['。', '！', '？']) {
        let cut_end = pos
            + sliced[pos..]
                .chars()
                .next()
                .map(|ch| ch.len_utf8())
                .unwrap_or(0);
        let cut = sliced[..cut_end].trim();
        if cut.chars().count() >= 200 {
            return Ok(cut.to_string());
        }
    }
    Ok(truncate_chars(text, ROLLUP_MAX_CHARS))
}

/// Record one completed run and refresh both memory files. The task note and
/// any rollup come from the run's own model. Failures are logged and do not
/// fail the agent run.
pub async fn record_completed_run(
    backend: &dyn crate::agent::llm::Backend,
    run_id: &str,
    run_dir: &Path,
    session_id: Option<&str>,
    task: &str,
    report: &str,
) {
    let note = match backend.summarize_for_memory(task, report).await {
        Ok(note) => note,
        Err(error) => {
            tracing::warn!(%error, "local memory note failed");
            return;
        }
    };
    if let Err(error) = store_note_and_fold(backend, run_id, run_dir, session_id, task, &note).await
    {
        tracing::warn!(%error, "local memory update failed");
    }
}

async fn store_note_and_fold(
    backend: &dyn crate::agent::llm::Backend,
    run_id: &str,
    run_dir: &Path,
    session_id: Option<&str>,
    task: &str,
    note: &str,
) -> anyhow::Result<()> {
    let run_id = run_id.trim();
    let note = collapse_ws(note);
    if run_id.is_empty() || note.is_empty() {
        return Ok(());
    }
    let dir = memories_dir();
    fs::create_dir_all(&dir)?;
    let entries_path = dir.join("entries.jsonl");
    let mut entries = read_entries(&entries_path)?;
    if !entries.iter().any(|entry| entry.run_id == run_id) {
        let run_dir = absolute_dir(run_dir);
        entries.push(MemoryEntry {
            run_id: run_id.to_string(),
            run_dir: run_dir.display().to_string(),
            session_id: session_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            task: truncate_chars(&collapse_ws(task), TASK_MAX_CHARS),
            finished_at_ms: now_ms(),
            note,
        });
        if entries.len() > MAX_STORED_ENTRIES {
            let drop_count = entries.len() - MAX_STORED_ENTRIES;
            entries.drain(0..drop_count);
        }
    }

    let state_path = dir.join("memory_state.json");
    let mut state = read_state(&state_path)?;
    fold_entries(backend, &entries, &mut state).await;

    write_memory_files(&dir, &entries, &state)?;
    Ok(())
}

/// Storage-independent folding, shared by local files and account-owned hosted storage.
pub async fn fold_entries(backend: &dyn crate::agent::llm::Backend, entries: &[MemoryEntry], state: &mut MemoryState) {
    let live_ids: BTreeSet<_> = entries.iter().map(|entry| entry.run_id.as_str()).collect();
    state
        .folded_run_ids
        .retain(|run_id| live_ids.contains(run_id.as_str()));

    loop {
        let pending = pending_entries(entries, state);
        if pending.len() < FOLD_BATCH {
            break;
        }
        let batch: Vec<&MemoryEntry> = pending.into_iter().take(FOLD_BATCH).collect();
        let batch_text = render_note_lines(&batch);
        match backend
            .compress_memory_history(&state.compressed, &batch_text)
            .await
        {
            Ok(compressed) => {
                state.compressed = compressed;
                for entry in batch {
                    state.folded_run_ids.push(entry.run_id.clone());
                }
            }
            Err(error) => {
                tracing::warn!(%error, "local memory rollup failed");
                break;
            }
        }
    }

}
pub fn summary_for_entries(entries: &[MemoryEntry], state: &MemoryState) -> String {
    render_summary(state, &pending_entries(entries, state))
}

fn pending_entries<'a>(entries: &'a [MemoryEntry], state: &MemoryState) -> Vec<&'a MemoryEntry> {
    let folded: BTreeSet<_> = state.folded_run_ids.iter().map(String::as_str).collect();
    entries
        .iter()
        .filter(|entry| !folded.contains(entry.run_id.as_str()))
        .collect()
}

fn write_memory_files(
    dir: &Path,
    entries: &[MemoryEntry],
    state: &MemoryState,
) -> std::io::Result<()> {
    atomic_write(&dir.join("entries.jsonl"), &render_entries(entries))?;
    atomic_write(&dir.join("memory_tasks.md"), &render_tasks(entries))?;
    atomic_write(
        &dir.join("memory_summary.md"),
        &render_summary(state, &pending_entries(entries, state)),
    )?;
    let state_json = serde_json::to_string_pretty(state)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    atomic_write(&dir.join("memory_state.json"), &(state_json + "\n"))?;
    Ok(())
}

fn read_entries(path: &Path) -> std::io::Result<Vec<MemoryEntry>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        let Ok(entry) = serde_json::from_str::<MemoryEntry>(line.trim()) else {
            continue;
        };
        if entry.run_id.is_empty() || !seen.insert(entry.run_id.clone()) {
            continue;
        }
        entries.push(entry);
    }
    Ok(entries)
}

fn read_state(path: &Path) -> std::io::Result<MemoryState> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(MemoryState::default()),
        Err(error) => Err(error),
    }
}

fn render_entries(entries: &[MemoryEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        if let Ok(line) = serde_json::to_string(entry) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

fn render_tasks(entries: &[MemoryEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    format!(
        "# Memory tasks\n\n{}\n",
        render_note_lines(&entries.iter().collect::<Vec<_>>())
    )
}

fn render_summary(state: &MemoryState, pending: &[&MemoryEntry]) -> String {
    let compressed = state.compressed.trim();
    let mut body = String::from("# Memory summary\n");
    if !compressed.is_empty() {
        body.push('\n');
        body.push_str(compressed);
        body.push('\n');
    }
    if !pending.is_empty() {
        body.push_str("\n## Latest\n\n");
        body.push_str(&render_note_lines(pending));
        body.push('\n');
    }
    if compressed.is_empty() && pending.is_empty() {
        return String::new();
    }
    body
}

fn render_note_lines(entries: &[&MemoryEntry]) -> String {
    entries
        .iter()
        .map(|entry| {
            format!(
                "- {}：{}",
                format_finished(entry.finished_at_ms),
                entry.note
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn absolute_dir(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    })
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Give the model the opening and the ending of a long report. The stored
/// note is the model's paragraph, not this window.
fn bounded_report(report: &str) -> String {
    let report = report.trim();
    let count = report.chars().count();
    if count <= REPORT_HEAD_CHARS + REPORT_TAIL_CHARS {
        return report.to_string();
    }
    let head_end = report
        .char_indices()
        .nth(REPORT_HEAD_CHARS)
        .map(|(index, _)| index)
        .unwrap_or(report.len());
    let tail_start = report
        .char_indices()
        .nth(count - REPORT_TAIL_CHARS)
        .map(|(index, _)| index)
        .unwrap_or(0);
    format!("{}\n…\n{}", &report[..head_end], &report[tail_start..])
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let end = text
        .char_indices()
        .nth(max_chars.saturating_sub(1))
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    format!("{}…", &text[..end])
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn format_finished(finished_at_ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(finished_at_ms as i64)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "unknown time".to_string())
}
