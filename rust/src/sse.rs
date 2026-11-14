use serde::Deserialize;
use serde_json::Value;

/// One event of the stream. Every line of every `sse/*.sse` fixture
/// deserializes, and nothing is swallowed: `kind()` is the JSON `type`.
/// (A placeholder shape: the port may make this an enum, as long as
/// `parse` and `kind` keep their meaning.)
#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub rest: Value,
}

impl Event {
    pub fn kind(&self) -> &str {
        &self.kind
    }
}

/// The JSON of every `data: ` line of a raw SSE body, one per line, the
/// way the bash loop strips them.
pub fn data_lines(raw: &str) -> Vec<&str> {
    raw.lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .collect()
}

/// One `data:` payload into an [`Event`].
pub fn parse(line: &str) -> Result<Event, serde_json::Error> {
    serde_json::from_str(line)
}
