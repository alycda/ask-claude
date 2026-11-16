//! The Managed Agents transport (CONCEPTS.md, Session): the one-call
//! requests `ma_call` in `lib/agent-api.bash` makes, and a session's event
//! stream, which `session_stream_open` in `lib/agent-turn.bash` opens before
//! anything is sent. [`Http`] speaks to the API, [`Replay`] stands in for
//! it with canned pages and recorded streams (`ASK_CLAUDE_REPLAY`), and
//! [`Logged`] writes each request body to `ASK_CLAUDE_REQUEST_LOG` first.

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use super::{Deferred, Reply};

/// `MA_BETA`: the `anthropic-beta` value of every sessions request.
pub const BETA: &str = "managed-agents-2026-04-01";

/// The two shapes of a sessions request.
pub trait Sessions {
    /// `ma_call <method> <path> [body]`: one request under `/v1`, `body`
    /// sent when given, and the response body whatever its status, for
    /// the caller to read an error from. An `Err` is a request that got no
    /// response, which the caller reports as `ma_call`'s `no_response`
    /// body.
    fn call(&mut self, method: &str, path: &str, body: Option<&str>) -> io::Result<String>;

    /// The session's event stream (`/v1/sessions/<id>/events/stream`), as
    /// a reader whose request starts on its first read, so the read on the
    /// worker thread (KTD3) is also where a connect or an open blocks. It
    /// delivers only what is emitted after it opens: the caller waits for
    /// the connected comment before it posts the events of a turn.
    fn stream(&mut self, session: &str) -> io::Result<Reply>;

    /// The bytes of the session file `id`, named `name` in the session's
    /// list, written to `to` (`session_files`'s download). An `Err` is a
    /// download that failed, an error status included (`curl --fail`);
    /// what reached `to` before it is the caller's to throw away.
    fn download(&mut self, id: &str, name: &str, to: &mut dyn Write) -> io::Result<()>;

    /// `file_upload`: `bytes` to POST /v1/files as the file `name`; the
    /// response body whatever its status, for the caller to read the id
    /// or the error from. An `Err` is a request that got no response.
    fn upload(&mut self, name: &str, bytes: &[u8]) -> io::Result<String>;
}

/// The content type curl gives a form part by its file's extension, case
/// aside (`ContentTypeForFilename` in curl's lib/mime.c); the Files API
/// keeps it as the file's MIME type.
fn content_type(name: &str) -> &'static str {
    const TYPES: [(&str, &str); 10] = [
        (".gif", "image/gif"),
        (".jpg", "image/jpeg"),
        (".jpeg", "image/jpeg"),
        (".png", "image/png"),
        (".svg", "image/svg+xml"),
        (".txt", "text/plain"),
        (".htm", "text/html"),
        (".html", "text/html"),
        (".pdf", "application/pdf"),
        (".xml", "application/xml"),
    ];
    let lower = name.to_ascii_lowercase();
    TYPES
        .iter()
        .find(|(ext, _)| lower.ends_with(ext))
        .map_or("application/octet-stream", |(_, t)| t)
}

/// The body `curl -F file=@<path>` sends, and its content type: one part
/// named `file` under `boundary`, the file's name with `"`, CR and LF
/// percent-encoded as curl's form escaping writes them.
fn form(boundary: &str, name: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    let filename = name
        .replace('"', "%22")
        .replace('\r', "%0D")
        .replace('\n', "%0A");
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {}\r\n\r\n",
        content_type(name)
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// A boundary in curl's shape (24 dashes, 22 letters and digits) that the
/// bytes do not contain, from the standard library's per-process random
/// hash keys: no random crate for one string.
fn boundary(bytes: &[u8]) -> String {
    use std::hash::{BuildHasher, Hasher};
    const ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    loop {
        let state = std::collections::hash_map::RandomState::new();
        let tail: String = (0..22u8)
            .map(|i| {
                let mut h = state.build_hasher();
                h.write_u8(i);
                ALNUM[(h.finish() % ALNUM.len() as u64) as usize] as char
            })
            .collect();
        let b = format!("{}{tail}", "-".repeat(24));
        if !bytes.windows(b.len()).any(|w| w == b.as_bytes()) {
            return b;
        }
    }
}

/// The API: `ma_call`'s curl and `session_stream_open`'s, with the headers
/// `ma_headers` sends. As in [`super::Http`], an error status is a body,
/// not an `Err`; no redirect is followed; 30 s to connect. A call has 120
/// s in all, as `curl --max-time 120`; a stream 1800 s, as the script's;
/// a download 600 s, as `session_files`'s.
pub struct Http {
    calls: ureq::Agent,
    streams: ureq::Agent,
    downloads: ureq::Agent,
    root: String,
    api_key: String,
}

impl Http {
    /// To the checked API root, one trailing slash dropped.
    pub fn new(base_url: &str, api_key: String) -> Self {
        let agent = |total: u64| {
            ureq::Agent::new_with_config(
                ureq::Agent::config_builder()
                    .http_status_as_error(false)
                    .max_redirects(0)
                    .timeout_global(Some(Duration::from_secs(total)))
                    .timeout_connect(Some(Duration::from_secs(30)))
                    .build(),
            )
        };
        Self {
            calls: agent(120),
            streams: agent(1800),
            downloads: agent(600),
            root: base_url.strip_suffix('/').unwrap_or(base_url).to_string(),
            api_key,
        }
    }
}

/// `ma_headers` on a request.
fn headers<B>(request: ureq::RequestBuilder<B>, api_key: &str) -> ureq::RequestBuilder<B> {
    request
        .header("content-type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2026-10-21")
        .header("anthropic-beta", BETA)
}

/// A path as a URI takes it: curl -g sends brackets as they are, the http
/// crate refuses them, and the API reads their escapes the same.
fn uri(root: &str, path: &str) -> String {
    format!("{root}/v1{}", path.replace('[', "%5B").replace(']', "%5D"))
}

/// A response body as text, whatever its status.
fn body_text(response: ureq::http::Response<ureq::Body>) -> io::Result<String> {
    let mut bytes = Vec::new();
    response.into_body().into_reader().read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

impl Sessions for Http {
    fn call(&mut self, method: &str, path: &str, body: Option<&str>) -> io::Result<String> {
        let url = uri(&self.root, path);
        let key = &self.api_key;
        let response = match (method, body) {
            ("GET", _) => headers(self.calls.get(&url), key).call(),
            ("POST", Some(body)) => headers(self.calls.post(&url), key).send(body),
            ("POST", None) => headers(self.calls.post(&url), key).send_empty(),
            _ => return Err(io::Error::other(format!("no {method} request here"))),
        }
        .map_err(io::Error::other)?;
        body_text(response)
    }

    fn stream(&mut self, session: &str) -> io::Result<Reply> {
        let url = uri(&self.root, &format!("/sessions/{session}/events/stream"));
        let (agent, key) = (self.streams.clone(), self.api_key.clone());
        Ok(Box::new(Deferred::Pending(Box::new(move || {
            let response = headers(agent.get(&url), &key)
                .call()
                .map_err(io::Error::other)?;
            Ok(Box::new(response.into_body().into_reader()) as Reply)
        }))))
    }

    /// GET /v1/files/<id>/content with the session headers; an error
    /// status fails as `curl --fail` fails. The id comes from the API and
    /// names a URL path, so one that is not an id goes nowhere.
    fn download(&mut self, id: &str, _name: &str, to: &mut dyn Write) -> io::Result<()> {
        if id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(io::Error::other("the file id is not one"));
        }
        let url = uri(&self.root, &format!("/files/{id}/content"));
        let response = headers(self.downloads.get(&url), &self.api_key)
            .call()
            .map_err(io::Error::other)?;
        let status = response.status().as_u16();
        if status >= 400 {
            return Err(io::Error::other(format!(
                "The requested URL returned error: {status}"
            )));
        }
        io::copy(&mut response.into_body().into_reader(), to)?;
        Ok(())
    }

    /// POST /v1/files as a form, with the key and the API version only:
    /// the Files API is out of beta, and the body is not JSON. The
    /// script's curl has no time limit here; this has the stream's.
    fn upload(&mut self, name: &str, bytes: &[u8]) -> io::Result<String> {
        let (content_type, body) = form(&boundary(bytes), name, bytes);
        let response = self
            .streams
            .post(format!("{}/v1/files", self.root))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2026-10-21")
            .header("content-type", content_type)
            .send(&body[..])
            .map_err(io::Error::other)?;
        body_text(response)
    }
}

/// `ASK_CLAUDE_REPLAY` for sessions. A call is answered from the replay
/// directory's canned pages, or `ma_call`'s defaults when the page is
/// missing or the replay is a file; the n-th stream this process opens is
/// the directory's `<n>.sse`, or the file itself, opened on the first read
/// (a FIFO's open blocks there, not on the main thread). A missing stream
/// file is a read that fails: no response.
pub struct Replay {
    path: PathBuf,
    streams: usize,
}

impl Replay {
    pub fn new(path: PathBuf) -> Self {
        Self { path, streams: 0 }
    }

    /// The canned page `name`, or `default`.
    fn page(&self, name: &str, default: &str) -> String {
        std::fs::read(self.path.join(name))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_else(|_| default.to_string())
    }
}

impl Sessions for Replay {
    /// The cases of `ma_call`'s replay branch, in its order.
    fn call(&mut self, method: &str, path: &str, body: Option<&str>) -> io::Result<String> {
        let session = path.strip_prefix("/sessions/");
        Ok(match method {
            "POST" if path == "/sessions" => {
                self.page("create.json", "{\"id\":\"sesn_FIXTURE01\",\"status\":\"running\"}\n")
            }
            "POST" if session.is_some_and(|p| p.ends_with("/archive")) => self.page(
                "archive.json",
                "{\"id\":\"sesn_FIXTURE01\",\"status\":\"terminated\",\"archived_at\":\"2026-11-05T00:00:00Z\"}\n",
            ),
            "POST" if session.is_some_and(|p| p.ends_with("/events")) => {
                self.page("events-post.json", "{}\n")
            }
            "POST" if session.is_some() => {
                // `jq -c '{id: "sesn_FIXTURE01"} + .'` over the body sent
                let echoed = || -> Option<String> {
                    let serde_json::Value::Object(sent) =
                        serde_json::from_str(body.unwrap_or("")).ok()?
                    else {
                        return None;
                    };
                    let mut o = serde_json::Map::new();
                    o.insert("id".into(), "sesn_FIXTURE01".into());
                    o.extend(sent);
                    Some(crate::tojson(&serde_json::Value::Object(o)) + "\n")
                };
                self.page("update.json", &echoed().unwrap_or_default())
            }
            "GET" if session.is_some_and(|p| p.contains("/events")) => {
                self.page("events.json", "{\"data\":[]}\n")
            }
            "GET" if path.starts_with("/sessions") => self.page("sessions.json", "{\"data\":[]}\n"),
            "GET" if path.starts_with("/files") => self.page("files.json", "{\"data\":[]}\n"),
            _ => "{}\n".to_string(),
        })
    }

    fn stream(&mut self, _session: &str) -> io::Result<Reply> {
        self.streams += 1;
        let file = if self.path.is_dir() {
            self.path.join(format!("{}.sse", self.streams))
        } else {
            self.path.clone()
        };
        Ok(Box::new(Deferred::Pending(Box::new(
            move || match std::fs::File::open(&file) {
                Ok(f) => Ok(Box::new(f) as Reply),
                Err(e) => Err(io::Error::new(e.kind(), format!("{}: {e}", file.display()))),
            },
        ))))
    }

    /// `<dir>/files/<name>`, as `cat` copies it: a file that is not there
    /// is said on stderr and leaves the download empty, not failed.
    fn download(&mut self, _id: &str, name: &str, to: &mut dyn Write) -> io::Result<()> {
        let file = self.path.join("files").join(name);
        match std::fs::File::open(&file) {
            Ok(mut f) => io::copy(&mut f, to).map(|_| ()),
            Err(e) => {
                eprintln!("ask-claude: {}: {e}", file.display());
                Ok(())
            }
        }
    }

    /// `file_upload`'s replay branch: the id it stands in for the API's.
    fn upload(&mut self, _name: &str, _bytes: &[u8]) -> io::Result<String> {
        Ok("{\"id\":\"file_FIXTURE01\"}\n".to_string())
    }
}

/// The request log around another sessions transport: each body `ma_call`
/// sends, one line, before it is sent (a call without a body writes
/// nothing); refused when another account owns or can read it, as
/// [`super::Logged`] refuses it.
pub struct Logged {
    inner: Box<dyn Sessions + Send>,
    requests: Option<PathBuf>,
}

impl Logged {
    /// `Err` is the refusal of a request log that is not private.
    pub fn new(inner: Box<dyn Sessions + Send>, requests: Option<PathBuf>) -> Result<Self, String> {
        super::refuse_shared(requests.as_deref())?;
        Ok(Self { inner, requests })
    }
}

impl Sessions for Logged {
    fn call(&mut self, method: &str, path: &str, body: Option<&str>) -> io::Result<String> {
        if let (Some(log), Some(body)) = (&self.requests, body.filter(|b| !b.is_empty())) {
            super::log_request(log, body)?;
        }
        self.inner.call(method, path, body)
    }

    fn stream(&mut self, session: &str) -> io::Result<Reply> {
        self.inner.stream(session)
    }

    fn download(&mut self, id: &str, name: &str, to: &mut dyn Write) -> io::Result<()> {
        self.inner.download(id, name, to)
    }

    /// An upload is logged as `{"file": <name>}`, the form being file
    /// contents and not a line of JSON. The script writes that line under
    /// replay only; here a live upload is logged the same way, so the log
    /// holds every request either way.
    fn upload(&mut self, name: &str, bytes: &[u8]) -> io::Result<String> {
        if let Some(log) = &self.requests {
            super::log_request(log, &crate::tojson(&serde_json::json!({ "file": name })))?;
        }
        self.inner.upload(name, bytes)
    }
}

/// The replay seam's directory or file, when one is set.
pub fn replay_path() -> Option<PathBuf> {
    std::env::var_os("ASK_CLAUDE_REPLAY")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn dir_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, body) in files {
            std::fs::write(dir.path().join(name), body).unwrap();
        }
        dir
    }

    fn read(mut r: Reply) -> io::Result<String> {
        let mut s = String::new();
        r.read_to_string(&mut s)?;
        Ok(s)
    }

    /// Each case of `ma_call`'s replay branch, its default and its page.
    #[test]
    fn replay_answers_each_call_as_ma_call_does() {
        let file = Path::new("/nonexistent/text.sse");
        let mut r = Replay::new(file.to_path_buf());
        let call = |r: &mut Replay, m: &str, p: &str, b: Option<&str>| r.call(m, p, b).unwrap();
        assert_eq!(
            call(&mut r, "POST", "/sessions", Some("{}")),
            "{\"id\":\"sesn_FIXTURE01\",\"status\":\"running\"}\n"
        );
        assert!(call(&mut r, "POST", "/sessions/sesn_X/archive", None).contains("terminated"));
        assert_eq!(
            call(&mut r, "POST", "/sessions/sesn_X/events", Some("{}")),
            "{}\n"
        );
        assert_eq!(
            call(
                &mut r,
                "POST",
                "/sessions/sesn_X",
                Some("{\"budget\":null}")
            ),
            "{\"id\":\"sesn_FIXTURE01\",\"budget\":null}\n"
        );
        assert_eq!(call(&mut r, "POST", "/sessions/sesn_X", Some("[]")), "");
        let types = "/sessions/sesn_X/events?types[]=user.message&types[]=agent.message";
        assert_eq!(call(&mut r, "GET", types, None), "{\"data\":[]}\n");
        assert_eq!(
            call(&mut r, "GET", "/sessions?order=desc&limit=50", None),
            "{\"data\":[]}\n"
        );
        assert_eq!(
            call(&mut r, "GET", "/files?scope_id=sesn_X", None),
            "{\"data\":[]}\n"
        );
        assert_eq!(call(&mut r, "DELETE", "/x", None), "{}\n");

        let dir = dir_with(&[
            ("create.json", "C"),
            ("events-post.json", "E"),
            ("update.json", "U"),
            ("events.json", "H"),
            ("sessions.json", "S"),
        ]);
        let mut r = Replay::new(dir.path().to_path_buf());
        assert_eq!(call(&mut r, "POST", "/sessions", Some("{}")), "C");
        assert_eq!(
            call(&mut r, "POST", "/sessions/sesn_X/events", Some("{}")),
            "E"
        );
        assert_eq!(call(&mut r, "POST", "/sessions/sesn_X", Some("{}")), "U");
        assert_eq!(call(&mut r, "GET", types, None), "H");
        assert_eq!(
            call(&mut r, "GET", "/sessions?order=desc&limit=50", None),
            "S"
        );
    }

    /// The n-th stream is `<n>.sse`; one past the last is a failed read.
    #[test]
    fn replay_serves_one_stream_file_per_open() {
        let dir = dir_with(&[("1.sse", "one"), ("2.sse", "two")]);
        let mut r = Replay::new(dir.path().to_path_buf());
        assert_eq!(read(r.stream("sesn_X").unwrap()).unwrap(), "one");
        assert_eq!(read(r.stream("sesn_X").unwrap()).unwrap(), "two");
        assert!(read(r.stream("sesn_X").unwrap()).is_err());
    }

    /// A body goes to the log before the call; a GET's absence of one
    /// writes nothing.
    #[test]
    fn logged_writes_each_body_and_nothing_for_none() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("requests.jsonl");
        let inner = Box::new(Replay::new(dir.path().join("none")));
        let mut t = Logged::new(inner, Some(log.clone())).unwrap();
        t.call("POST", "/sessions", Some("{\"a\":1}")).unwrap();
        t.call("GET", "/sessions?order=desc&limit=50", None)
            .unwrap();
        t.call("POST", "/sessions/sesn_X/events", Some("{\"b\":2}"))
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "{\"a\":1}\n{\"b\":2}\n"
        );
    }
}
