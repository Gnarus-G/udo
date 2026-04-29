use anyhow::Result;
use notify_rust::Notification;

pub fn success(body: &str) -> Result<()> {
    Notification::new()
        .summary("udo: done")
        .body(body)
        .icon("dialog-information")
        .appname("udo")
        .show()?;
    Ok(())
}

pub fn failure(body: &str) -> Result<()> {
    Notification::new()
        .summary("udo: failed")
        .body(body)
        .icon("dialog-error")
        .appname("udo")
        .show()?;
    Ok(())
}
