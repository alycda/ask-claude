//! Chats on disk under the State directory (`lib/chat.bash`, and the
//! state directory lines of `lib/config.bash`): the directory and its
//! checks, a new Chat, private writes, the container file, `list` and the
//! chat `resume` picks.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

/// `$XDG_STATE_HOME/ask-claude`, or `~/.local/state/ask-claude`, created
/// and checked by [`private_state`].
pub fn state_dir() -> Result<PathBuf, String> {
    let base = match std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        // `$HOME/.local/state` under `set -u`: no HOME stops the script, and
        // an empty one names the root, never a directory under the cwd
        None => match std::env::var_os("HOME") {
            Some(mut home) => {
                home.push("/.local/state");
                PathBuf::from(home)
            }
            None => return Err("HOME: unbound variable; set HOME or XDG_STATE_HOME".into()),
        },
    };
    let state = base.join("ask-claude");
    private_state(&state)?;
    Ok(state)
}

/// The checks `lib/config.bash` runs on the state directory, in its order
/// and words: created if missing, then refused when it is a symlink (its
/// target is not ours to vouch for), when it cannot be made private (chmod
/// 700), when another account owns it, or when others can still write it.
/// Fixed names are written there, so a directory another account controls
/// could have them planted (CODING_STANDARDS.md, files and state).
///
/// Unlike the script, the chmod and the stat act on one open directory,
/// opened without following a link, so a name swapped between the checks
/// cannot send them elsewhere. Later opens still go by name: an ancestor
/// another account can write is the audit's open lead, which this does not
/// close.
#[cfg(unix)]
pub fn private_state(state: &Path) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

    let shown = state.display();
    let symlink = || {
        format!("{shown} is a symlink; keep the history in a private directory, not a link to one")
    };
    let cannot_create = || format!("cannot create {shown}");
    let not_private = || {
        format!("cannot make {shown} private (chmod 700); keep the history in a directory you own")
    };
    let private = || std::fs::Permissions::from_mode(0o700);

    std::fs::create_dir_all(state).map_err(|_| cannot_create())?;
    if std::fs::symlink_metadata(state).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(symlink());
    }
    let open = || {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(state)
    };
    let dir = match open() {
        // a directory of ours we cannot read: chmod it by name (it was no
        // link a moment ago) and open it again, as the script's chmod would
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            std::fs::set_permissions(state, private()).map_err(|_| not_private())?;
            open()
        }
        other => other,
    };
    let dir = dir.map_err(|e| match e.raw_os_error() {
        Some(libc::ELOOP) => symlink(),
        _ => cannot_create(),
    })?;
    dir.set_permissions(private()).map_err(|_| not_private())?;
    let meta = dir.metadata().map_err(|_| not_private())?;
    // SAFETY: geteuid(2) takes nothing and cannot fail
    let me = unsafe { libc::geteuid() };
    verdict(state, me, meta.uid(), meta.mode())
}

#[cfg(not(unix))]
pub fn private_state(state: &Path) -> Result<(), String> {
    std::fs::create_dir_all(state).map_err(|_| format!("cannot create {}", state.display()))
}

/// The owner and mode half of [`private_state`], over what the directory's
/// stat said: `me` is this process's effective uid. The write bits are the
/// group's and others', the two the script cuts from `ls -ldn`.
#[cfg_attr(not(unix), allow(dead_code))]
fn verdict(state: &Path, me: u32, owner: u32, mode: u32) -> Result<(), String> {
    if owner != me {
        return Err(format!(
            "{} is owned by uid {owner}, not you; keep the history in a directory you own",
            state.display()
        ));
    }
    if mode & 0o022 != 0 {
        return Err(format!(
            "{} is writable by other accounts; keep the history in a private directory",
            state.display()
        ));
    }
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
pub(crate) fn stamp() -> String {
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
pub(crate) fn stamp() -> String {
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

/// A new file private to the user (mode 0600) named `<prefix><pid>.<n>`,
/// created by this call and never opened when the name exists, as
/// `mktemp` creates one: nothing planted beforehand, a symlink above all,
/// can redirect what is written to it.
pub fn fresh(prefix: &Path) -> io::Result<(PathBuf, File)> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    let mut n = 0u32;
    loop {
        let mut name = prefix.as_os_str().to_owned();
        name.push(format!("{}.{n}", std::process::id()));
        let tmp = PathBuf::from(name);
        match opts.open(&tmp) {
            Ok(file) => return Ok((tmp, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && n < 100 => n += 1,
            Err(e) => return Err(e),
        }
    }
}

/// Put `bytes` at `path` without writing through the name: a new private
/// file beside it (mode 0600, created, never opened if it exists), then
/// one rename over the name, which replaces what is there and follows no
/// symlink (CODING_STANDARDS.md, files and state).
pub fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut prefix = path.as_os_str().to_owned();
    prefix.push(".tmp.");
    let (tmp, mut file) = fresh(Path::new(&prefix))?;
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

/// The Chat as `you:` / `claude:` text, `show_history` in
/// `lib/chat.bash`: a turn's content when it is a string, else its text
/// blocks joined by newlines (thinking and tool blocks skipped); each turn
/// through `safe`, then the blank line `jq -r` adds.
pub fn text(turns: &Value) -> String {
    turns
        .as_array()
        .into_iter()
        .flatten()
        .map(|turn| {
            let who = if turn["role"] == "user" {
                "you"
            } else {
                "claude"
            };
            let said = match &turn["content"] {
                Value::String(s) => s.clone(),
                Value::Array(blocks) => blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .map(|b| match &b["text"] {
                        Value::String(s) => s.clone(),
                        Value::Null => String::new(),
                        other => other.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            };
            crate::stream::safe(&format!("{who}: {said}\n")) + "\n"
        })
        .collect()
}

/// Remove a Chat and its container file: an empty Chat leaves no file.
pub fn forget(chat: &Path) {
    let _ = std::fs::remove_file(chat);
    let _ = std::fs::remove_file(container_file(chat));
}

/// A Chat's name, its file name without `.json`, as `list` prints it and
/// `resume` takes it.
pub fn name(chat: &Path) -> String {
    let file = chat.file_name().unwrap_or_default().to_string_lossy();
    file.strip_suffix(".json").unwrap_or(&file).to_string()
}

/// The Chats on disk, newest first: `ls -1t "$STATE"/*.json`, so no
/// hidden file and nothing but `.json` (the event log is `.jsonl`), ties
/// by name. Only regular files: the script's next step, `jq`, reads each.
fn chats(state: &Path) -> Vec<PathBuf> {
    let Ok(dir) = std::fs::read_dir(state) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, String, PathBuf)> = dir
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if name.starts_with('.') || !name.ends_with(".json") {
                return None;
            }
            let path = entry.path();
            let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file())?;
            Some((meta.modified().ok()?, name, path))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    found.into_iter().map(|(_, _, path)| path).collect()
}

/// A Chat's turns as stored, or None when the file is not JSON.
pub fn load(chat: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(chat).ok()?).ok()
}

/// `jq length`: what the script counts as a Chat's turns. A number's is
/// its absolute value, so a Chat file holding `5` has 5; a boolean has
/// none, the error the script's `$(...)` turns into nothing.
fn length(v: &Value) -> Option<Value> {
    match v {
        Value::Array(a) => Some(a.len().into()),
        Value::Object(o) => Some(o.len().into()),
        Value::String(s) => Some(s.chars().count().into()),
        Value::Null => Some(0.into()),
        Value::Number(n) => Some(match (n.as_u64(), n.as_i64()) {
            (Some(n), _) => n.into(),
            (None, Some(n)) => n.unsigned_abs().into(),
            _ => n.as_f64().map(f64::abs).into(),
        }),
        Value::Bool(_) => None,
    }
}

/// `list`, `list_chats` in `lib/chat.bash`: one line per Chat, newest
/// first: its name, its turn count, and its first prompt cut at 60
/// characters (nothing when that turn is not plain text), through `safe`.
pub fn list(state: &Path) -> String {
    let mut out = String::new();
    for chat in chats(state) {
        let turns = load(&chat);
        let count = turns
            .as_ref()
            .and_then(length)
            .map_or(String::new(), |n| n.to_string());
        let first = turns
            .as_ref()
            .and_then(Value::as_array)
            .and_then(|t| t.iter().find(|turn| turn["role"] == "user"))
            .and_then(|turn| turn["content"].as_str())
            .map_or(String::new(), |s| {
                crate::stream::safe(&s.chars().take(60).collect::<String>())
            });
        out.push_str(&format!("{}  {count} turns  {first}\n", name(&chat)));
    }
    out
}

/// The Chat `resume` continues, `find_chat` in `lib/chat.bash`: the one
/// whose name starts with `name`, or with no name the newest that has
/// turns (an empty file from an aborted start is skipped). `Err` is the
/// script's message for exit 2. A name is a name: a path part or a glob
/// character is refused, so it never leaves the state directory.
pub fn find(state: &Path, name: Option<&str>) -> Result<PathBuf, String> {
    let nothing = || "nothing to resume (see `ask-claude list`)".to_string();
    let Some(name) = name else {
        return chats(state)
            .into_iter()
            // `[ "$(jq length "$f")" -gt 0 ]`: a whole number above 0
            .find(|chat| {
                load(chat)
                    .as_ref()
                    .and_then(length)
                    .and_then(|n| n.as_u64())
                    .is_some_and(|n| n > 0)
            })
            .ok_or_else(nothing);
    };
    if name.contains(['/', '*', '?', '[']) {
        return Err("resume takes a chat name from `ask-claude list`".into());
    }
    // the glob "$STATE/$name"*.json: the name, anything, then .json
    let matches: Vec<PathBuf> = chats(state)
        .into_iter()
        .filter(|chat| {
            let file = chat.file_name().unwrap_or_default().to_string_lossy();
            file.len() >= name.len() + ".json".len() && file.starts_with(name)
        })
        .collect();
    match matches.len() {
        0 => Err(nothing()),
        1 => Ok(matches.into_iter().next().unwrap_or_default()),
        n => Err(format!(
            "\"{name}\" matches {n} chats, see `ask-claude list`"
        )),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    fn me() -> u32 {
        // SAFETY: geteuid(2) takes nothing and cannot fail
        unsafe { libc::geteuid() }
    }

    #[test]
    fn a_missing_state_directory_is_created_private() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("state/ask-claude");
        private_state(&state).unwrap();
        assert_eq!(mode(&state), 0o700);
    }

    #[test]
    fn an_open_directory_of_our_own_is_made_private() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("ask-claude");
        std::fs::create_dir(&state).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o777)).unwrap();
        private_state(&state).unwrap();
        assert_eq!(mode(&state), 0o700);
    }

    /// The link is refused before anything is done through it: the target
    /// keeps its mode, as `[ ! -L "$STATE" ]` comes before the chmod.
    #[test]
    fn a_symlink_is_refused_and_its_target_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("elsewhere");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        let state = tmp.path().join("ask-claude");
        std::os::unix::fs::symlink(&target, &state).unwrap();
        assert_eq!(
            private_state(&state),
            Err(format!(
                "{} is a symlink; keep the history in a private directory, not a link to one",
                state.display()
            ))
        );
        assert_eq!(mode(&target), 0o755);
    }

    #[test]
    fn a_file_where_the_directory_goes_cannot_be_created() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("ask-claude");
        std::fs::write(&state, "").unwrap();
        assert_eq!(
            private_state(&state),
            Err(format!("cannot create {}", state.display()))
        );
    }

    /// A directory another account owns: chmod fails for anyone but root,
    /// and that failure is the refusal. `/` is such a directory on every
    /// unix; as root the chmod would succeed, so the case is not run there.
    #[test]
    fn a_directory_whose_mode_cannot_be_set_is_refused() {
        if me() == 0 {
            eprintln!("skipped: as root chmod succeeds on any directory");
            return;
        }
        assert_eq!(
            private_state(Path::new("/")),
            Err(
                "cannot make / private (chmod 700); keep the history in a directory you own".into()
            )
        );
    }

    #[test]
    fn another_owner_is_refused_with_the_bash_message() {
        let p = Path::new("/state/ask-claude");
        assert_eq!(
            verdict(p, 1000, 65534, 0o40700),
            Err(
                "/state/ask-claude is owned by uid 65534, not you; keep the history in a directory you own"
                    .into()
            )
        );
    }

    /// Reachable when the chmod succeeded and the mode did not take, as on
    /// a file system that ignores it; the script's `cut -c6,9` reads the
    /// group and other write bits.
    #[test]
    fn a_directory_writable_by_group_or_others_is_refused_with_the_bash_message() {
        let p = Path::new("/state/ask-claude");
        let refused = Err(
            "/state/ask-claude is writable by other accounts; keep the history in a private directory"
                .into(),
        );
        assert_eq!(verdict(p, 1000, 1000, 0o40770), refused);
        assert_eq!(verdict(p, 1000, 1000, 0o40702), refused);
        assert_eq!(verdict(p, 1000, 1000, 0o40750), Ok(()));
        assert_eq!(verdict(p, 1000, 1000, 0o40700), Ok(()));
    }
}
