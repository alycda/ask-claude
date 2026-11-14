//! Chats on disk under the State directory (`lib/chat.bash`, and the
//! state directory lines of `lib/config.bash`). U4 holds the one shot's
//! part; `list`, `resume`, the container file and the owner, symlink and
//! mode checks on the directory are U5's.

use std::path::PathBuf;

/// `$XDG_STATE_HOME/ask-claude`, or `~/.local/state/ask-claude`, created
/// private (`mkdir -p` then `chmod 700`). The message on failure is the
/// script's.
pub fn state_dir() -> Result<PathBuf, String> {
    let base = match std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"),
    };
    let state = base.join("ask-claude");
    std::fs::create_dir_all(&state).map_err(|_| format!("cannot create {}", state.display()))?;
    private_dir(&state).map_err(|_| {
        format!(
            "cannot make {} private (chmod 700); keep the history in a directory you own",
            state.display()
        )
    })?;
    Ok(state)
}

#[cfg(unix)]
fn private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn private_dir(_dir: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}
