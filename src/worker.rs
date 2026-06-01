use anyhow::Result;

use crate::agent;
use crate::notify;
use crate::ollama::Client;
use crate::progress::{Event, ProgressWriter};
use crate::paths;
use crate::text;

const NOTIFY_BODY_MAX: usize = 200;

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
            let msg = format!("client init failed: {e}");
            eprintln!("{msg}");
            fail(&mut pw, &msg, &log_str);
            std::process::exit(1);
        }
    };

    match agent::run(&client, &model, &prompt, &mut pw).await {
        Ok(summary) => {
            let _ = pw.emit(Event::Completed {
                summary: summary.clone(),
            });
            let body = format!("{} (log: {log_str})", text::truncate(&summary, NOTIFY_BODY_MAX));
            if let Err(e) = notify::success(&body) {
                eprintln!("notify failed: {e:#}");
                std::process::exit(2);
            }
            Ok(())
        }
        Err(e) => {
            let msg = format!("{e}");
            eprintln!("agent failed: {e:#}");
            fail(&mut pw, &msg, &log_str);
            std::process::exit(1);
        }
    }
}

fn fail(pw: &mut ProgressWriter, msg: &str, log_str: &str) {
    let _ = pw.emit(Event::Failed {
        error: msg.to_string(),
    });
    let body = format!("{}\nlog: {}", text::truncate(msg, NOTIFY_BODY_MAX), log_str);
    let _ = notify::failure(&body);
}
