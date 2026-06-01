use anyhow::Result;

use crate::agent;
use crate::notify;
use crate::ollama::Client;
use crate::progress::{Event, ProgressWriter};
use crate::paths;

pub async fn run(task_id: String, prompt: String, model: String) -> Result<()> {
    let log_path = paths::log_file(&task_id).ok();
    let log_str = log_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".into());

    let mut pw = ProgressWriter::create(&task_id)?;
    pw.emit(Event::Started {
        task_id: task_id.clone(),
        model: model.clone(),
        prompt: prompt.clone(),
    })?;

    println!("task_id={task_id}");
    println!("model={model}");
    println!("prompt={prompt}");

    let client = match Client::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("client init failed: {e:#}");
            let _ = pw.emit(Event::Failed {
                error: format!("client init failed: {e}"),
            });
            let _ = notify::failure(&format!("client init failed: {e}\nlog: {log_str}"));
            std::process::exit(1);
        }
    };

    match agent::run(&client, &model, &prompt, &mut pw).await {
        Ok(summary) => {
            let body = trim_for_notif(&summary);
            let _ = pw.emit(Event::Completed {
                summary: summary.clone(),
            });
            if let Err(e) = notify::success(&format!("{} (log: {log_str})", &body)) {
                eprintln!("notify failed: {e:#}");
                std::process::exit(2);
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("agent failed: {e:#}");
            let _ = pw.emit(Event::Failed {
                error: format!("{e}"),
            });
            let body = format!("{}\nlog: {}", trim_for_notif(&format!("{e}")), log_str);
            let _ = notify::failure(&body);
            std::process::exit(1);
        }
    }
}

fn trim_for_notif(s: &str) -> String {
    const MAX: usize = 200;
    let s = s.trim();
    if s.chars().count() <= MAX {
        return s.to_string();
    }
    let mut out: String = s.chars().take(MAX).collect();
    out.push_str("…");
    out
}