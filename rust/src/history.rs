//! Chats on disk under the State directory (`lib/chat.bash`, and the
//! state directory lines of `lib/config.bash`). U4 holds the one shot's
//! part: the directory, a new Chat, private writes; `list`, `resume`, the
//! rest of the container file and the owner, symlink and mode checks on
//! the directory are U5's.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

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

/// A fresh Chat's path, `<state>/<YYYYmmddTHHMMSS>-<pid>.json` in local
/// time, as `new_chat` names it; the process id keeps two processes
/// started in the same second apart, and a name taken in this second waits
/// for the next.
pub fn new_chat(state: &Path) -> PathBuf {
    loop {
        let path = state.join(format!("{}-{}.json", stamp(), std::process::id()));
        if !path.exists() {
            return path;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// `date +%Y%m%dT%H%M%S`.
#[cfg(unix)]
fn stamp() -> String {
    // SAFETY: time(2) with no out-pointer, and localtime_r(3) into a tm
    // this frame owns; a null return leaves the zeroed tm, never read
    // uninitialised.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm
    };
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(not(unix))]
fn stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    secs.to_string()
}

/// The skills container's file beside a Chat, `$HIST.container`.
pub fn container_file(chat: &Path) -> PathBuf {
    let mut name = chat.as_os_str().to_owned();
    name.push(".container");
    PathBuf::from(name)
}

/// The Chat's turns, written the way jq writes them (two-space indent, a
/// final newline).
pub fn save(chat: &Path, turns: &Value) -> io::Result<()> {
    let mut text = serde_json::to_string_pretty(turns).map_err(io::Error::other)?;
    text.push('\n');
    replace(chat, text.as_bytes())
}

/// Put `bytes` at `path` without writing through the name: a new private
/// file beside it (mode 0600, created, never opened if it exists), then
/// one rename over the name, which replaces what is there and follows no
/// symlink (CODING_STANDARDS.md, files and state).
pub fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    let mut n = 0u32;
    let (tmp, mut file) = loop {
        let mut name = path.as_os_str().to_owned();
        name.push(format!(".tmp.{}.{n}", std::process::id()));
        let tmp = PathBuf::from(name);
        match opts.open(&tmp) {
            Ok(file) => break (tmp, file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && n < 100 => n += 1,
            Err(e) => return Err(e),
        }
    };
    let written = file.write_all(bytes);
    drop(file);
    match written.and_then(|()| std::fs::rename(&tmp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}
