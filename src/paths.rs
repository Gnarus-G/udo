use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

pub fn state_dir() -> Result<PathBuf> {
    let base = dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/state")))
        .context("could not determine state dir")?;
    let dir = base.join("udo");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

fn ensure_subdir(name: &str) -> Result<PathBuf> {
    let dir = state_dir()?.join(name);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

pub fn log_dir() -> Result<PathBuf> {
    ensure_subdir("log")
}

pub fn task_dir() -> Result<PathBuf> {
    ensure_subdir("tasks")
}

pub fn log_file(task_id: &str) -> Result<PathBuf> {
    Ok(log_dir()?.join(format!("{task_id}.log")))
}

pub fn task_file(task_id: &str) -> Result<PathBuf> {
    Ok(task_dir()?.join(format!("{task_id}.jsonl")))
}