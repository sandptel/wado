//! Setting the host clipboard with `wl-copy`. The desktop's own `wl-paste --watch cliphist store`
//! then puts it in the history.

use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use wado_compositor::clipboard::Clip;

pub async fn set(clip: &Clip) -> Result<(), String> {
    let mime = if clip.is_text() {
        "text/plain;charset=utf-8"
    } else {
        clip.mime.as_str()
    };
    // stdout and stderr to null: wl-copy forks a child that keeps serving the clipboard, and a
    // pipe it inherited would never reach EOF.
    let mut child = super::tool::command("wl-copy")?
        .args(["--type", mime])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false)
        .spawn()
        .map_err(|e| format!("wl-copy is not available here ({e})"))?;
    if let Some(mut i) = child.stdin.take() {
        i.write_all(&clip.data)
            .await
            .map_err(|e| format!("wl-copy: {e}"))?;
    }
    let ok = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
        .await
        .map_err(|_| "wl-copy took too long".to_string())?
        .map_err(|e| format!("wl-copy: {e}"))?
        .success();
    if ok {
        Ok(())
    } else {
        Err("wl-copy failed".into())
    }
}

/// What is on the host clipboard now: text if offered, else the first image type.
pub async fn current() -> Result<Clip, String> {
    let types = super::tool::run("wl-paste", &["--list-types"], b"").await?;
    let types = String::from_utf8_lossy(&types);
    let image = types.lines().find(|t| t.starts_with("image/"));
    let text = types
        .lines()
        .any(|t| t.starts_with("text/plain") || t == "UTF8_STRING");
    let mime = if text {
        "text/plain;charset=utf-8".to_string()
    } else if let Some(i) = image {
        i.to_string()
    } else {
        return Err("nothing we can pass on".into());
    };
    let data = super::tool::run("wl-paste", &["--no-newline", "--type", &mime], b"").await?;
    Ok(Clip {
        mime,
        data: data.into(),
    })
}
