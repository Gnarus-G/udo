use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use crate::paths;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "kind")]
pub enum Event {
    #[serde(rename = "started")]
    Started {
        task_id: String,
        model: String,
        prompt: String,
    },
    #[serde(rename = "iteration_started")]
    IterationStarted { iter: usize },
    #[serde(rename = "model_request_started")]
    ModelRequestStarted { iter: usize },
    #[serde(rename = "model_response_received")]
    ModelResponseReceived { iter: usize },
    #[serde(rename = "tool_started")]
    ToolStarted { iter: usize, command: String },
    #[serde(rename = "tool_finished")]
    ToolFinished {
        iter: usize,
        command: String,
        exit_code: i32,
        stdout_len: usize,
        stderr_len: usize,
    },
    #[serde(rename = "completed")]
    Completed { summary: String },
    #[serde(rename = "failed")]
    Failed { error: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TimedEvent {
    pub ts: DateTime<Utc>,
    #[serde(flatten)]
    pub event: Event,
}

pub struct ProgressWriter {
    file: File,
}

impl ProgressWriter {
    pub fn create(task_id: &str) -> Result<Self> {
        let path = paths::task_file(task_id)?;
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { file })
    }

    pub fn emit(&mut self, event: Event) -> Result<()> {
        let te = TimedEvent {
            ts: Utc::now(),
            event,
        };
        let mut line = serde_json::to_string(&te)?;
        line.push('\n');
        self.file.write_all(line.as_bytes())?;
        self.file.flush()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TaskState {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug)]
pub struct TaskSnapshot {
    pub task_id: String,
    pub state: TaskState,
    pub model: String,
    pub prompt: String,
    pub iter: usize,
    pub current_command: Option<String>,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

pub fn read_events(task_id: &str) -> Result<Vec<TimedEvent>> {
    let path = paths::task_file(task_id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(&path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(te) = serde_json::from_str::<TimedEvent>(line) {
            events.push(te);
        }
    }
    Ok(events)
}

pub fn snapshot(task_id: &str) -> Option<TaskSnapshot> {
    let events = read_events(task_id).ok()?;
    let mut snap = TaskSnapshot {
        task_id: task_id.to_string(),
        state: TaskState::Running,
        model: String::new(),
        prompt: String::new(),
        iter: 0,
        current_command: None,
        summary: None,
        error: None,
        started_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
    };
    if events.is_empty() {
        return None;
    }
    snap.started_at = events[0].ts;
    snap.updated_at = events[0].ts;
    for te in &events {
        snap.updated_at = te.ts;
        match &te.event {
            Event::Started {
                model,
                prompt,
                task_id: tid,
            } => {
                snap.model = model.clone();
                snap.prompt = prompt.clone();
                snap.task_id = tid.clone();
            }
            Event::IterationStarted { iter } => {
                snap.iter = *iter;
                snap.current_command = None;
            }
            Event::ModelRequestStarted { .. } => {}
            Event::ModelResponseReceived { .. } => {}
            Event::ToolStarted { command, .. } => {
                snap.current_command = Some(command.clone());
            }
            Event::ToolFinished { .. } => {
                snap.current_command = None;
            }
            Event::Completed { summary } => {
                snap.state = TaskState::Completed;
                snap.summary = Some(summary.clone());
                snap.completed_at = Some(te.ts);
            }
            Event::Failed { error } => {
                snap.state = TaskState::Failed;
                snap.error = Some(error.clone());
                snap.completed_at = Some(te.ts);
            }
        }
    }
    Some(snap)
}

pub fn list_task_ids() -> Result<Vec<String>> {
    let dir = paths::task_dir()?;
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "jsonl") {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                ids.push(name.to_string());
            }
        }
    }
    ids.sort();
    Ok(ids)
}

#[derive(Serialize)]
pub struct EwwStatus {
    pub running_count: usize,
    pub recent_count: usize,
    pub has_visible: bool,
    pub worst_state: String,
    pub latest: Option<LatestTask>,
}

#[derive(Serialize)]
pub struct LatestTask {
    pub task_id: String,
    pub state: String,
    pub summary: String,
    pub updated_secs_ago: i64,
}

const VISIBILITY_SECS: i64 = 900;

pub fn eww_status() -> EwwStatus {
    let Ok(ids) = list_task_ids() else {
        return EwwStatus {
            running_count: 0,
            recent_count: 0,
            has_visible: false,
            worst_state: String::new(),
            latest: None,
        };
    };
    let now = Utc::now();
    let mut running = 0usize;
    let mut recent = 0usize;
    let mut worst = String::new();
    let mut latest: Option<LatestTask> = None;
    for id in &ids {
        let Some(snap) = snapshot(id) else {
            continue;
        };
        let age = (now - snap.updated_at).num_seconds();
        let visible = age <= VISIBILITY_SECS;
        match snap.state {
            TaskState::Running => {
                running += 1;
                if visible {
                    recent += 1;
                    if worst != "running" {
                        worst = "running".to_string();
                    }
                }
            }
            TaskState::Failed => {
                if visible {
                    recent += 1;
                    if worst != "running" {
                        worst = "failed".to_string();
                    }
                }
            }
            TaskState::Completed => {
                if visible {
                    recent += 1;
                    if worst.is_empty() {
                        worst = "completed".to_string();
                    }
                }
            }
        }
        match &mut latest {
            None => {
                latest = Some(LatestTask {
                    task_id: snap.task_id.clone(),
                    state: format!("{:?}", snap.state).to_lowercase(),
                    summary: snap
                        .summary
                        .or(snap.error)
                        .or(snap.current_command.clone())
                        .unwrap_or_else(|| snap.prompt.chars().take(60).collect()),
                    updated_secs_ago: age,
                });
            }
            Some(l) => {
                if snap.updated_at > now - chrono::Duration::seconds(l.updated_secs_ago) {
                    l.task_id = snap.task_id.clone();
                    l.state = format!("{:?}", snap.state).to_lowercase();
                    l.summary = snap
                        .summary
                        .or(snap.error)
                        .or(snap.current_command.clone())
                        .unwrap_or_else(|| snap.prompt.chars().take(60).collect());
                    l.updated_secs_ago = age;
                }
            }
        }
    }
    if worst.is_empty() && latest.is_some() {
        worst = "completed".to_string();
    }
    EwwStatus {
        running_count: running,
        recent_count: recent,
        has_visible: recent > 0 || running > 0,
        worst_state: worst,
        latest,
    }
}