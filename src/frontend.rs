use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use crate::paths;

pub fn run(prompt: String, model: String) -> Result<()> {
    let task_id = format!("{:04x}", rand::random::<u16>());
    let log_path = paths::log_file(&task_id)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("opening log {}", log_path.display()))?;
    let log_err = log.try_clone()?;

    let exe = std::env::current_exe().context("locating current exe")?;

    let mut cmd = Command::new(exe);
    cmd.arg("--__worker")
        .arg("--task-id")
        .arg(&task_id)
        .arg("--model")
        .arg(&model)
        .arg(&prompt)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));

    unsafe {
        cmd.pre_exec(|| {
            // Detach from controlling terminal so the worker survives shell exit.
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = cmd.spawn().context("spawning worker")?;

    println!(
        "udo: task {} started (pid {}, log {})",
        task_id,
        child.id(),
        log_path.display()
    );
    println!("  watch with: udo watch {}", task_id);
    Ok(())
}
