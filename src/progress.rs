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
    let mut entries: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
        .filter_map(|e| {
            let mtime = e.metadata().ok()?.modified().ok()?;
            let name = e.path().file_stem()?.to_str()?.to_string();
            Some((mtime, name))
        })
        .collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(entries.into_iter().map(|(_, name)| name).collect())
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

#[derive(Clone, Copy, Eq, PartialEq)]
enum Worst {
    Completed,
    Failed,
    Running,
}

impl PartialOrd for Worst {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Worst {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        rank(*self).cmp(&rank(*other))
    }
}

fn rank(w: Worst) -> u8 {
    match w {
        Worst::Completed => 0,
        Worst::Failed => 1,
        Worst::Running => 2,
    }
}

#[derive(Default)]
struct Aggregate {
    running_count: usize,
    recent_count: usize,
    worst: Option<Worst>,
    latest: Option<LatestTask>,
}

impl Aggregate {
    fn fold(&mut self, snap: TaskSnapshot, now: DateTime<Utc>) {
        let age = (now - snap.updated_at).num_seconds();
        let visible = age <= VISIBILITY_SECS;
        let worst = match snap.state {
            TaskState::Running => {
                self.running_count += 1;
                if visible {
                    self.recent_count += 1;
                    Some(Worst::Running)
                } else {
                    None
                }
            }
            TaskState::Failed => {
                if visible {
                    self.recent_count += 1;
                    Some(Worst::Failed)
                } else {
                    None
                }
            }
            TaskState::Completed => {
                if visible {
                    self.recent_count += 1;
                    Some(Worst::Completed)
                } else {
                    None
                }
            }
        };
        if let Some(w) = worst {
            self.worst = Some(match self.worst {
                Some(cur) => cur.max(w),
                None => w,
            });
        }
        let replace = self
            .latest
            .as_ref()
            .is_none_or(|l| snap.updated_at > now - chrono::Duration::seconds(l.updated_secs_ago));
        if replace {
            let summary = snap
                .summary
                .clone()
                .or_else(|| snap.error.clone())
                .or_else(|| snap.current_command.clone())
                .unwrap_or_else(|| snap.prompt.chars().take(60).collect());
            self.latest = Some(LatestTask {
                task_id: snap.task_id,
                state: format!("{:?}", snap.state).to_lowercase(),
                summary,
                updated_secs_ago: age,
            });
        }
    }

    fn finish(self) -> EwwStatus {
        let worst_state = self
            .worst
            .map(|w| match w {
                Worst::Running => "running",
                Worst::Failed => "failed",
                Worst::Completed => "completed",
            })
            .map(str::to_string)
            .or_else(|| self.latest.as_ref().map(|_| "completed".to_string()))
            .unwrap_or_default();
        EwwStatus {
            running_count: self.running_count,
            recent_count: self.recent_count,
            has_visible: self.recent_count > 0 || self.running_count > 0,
            worst_state,
            latest: self.latest,
        }
    }
}

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
    let mut agg = Aggregate::default();
    for id in &ids {
        if let Some(snap) = snapshot(id) {
            agg.fold(snap, now);
        }
    }
    agg.finish()
}
