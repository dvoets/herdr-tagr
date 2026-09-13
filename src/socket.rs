//! Minimal client for the herdr socket API.
//!
//! Two shapes of connection, both newline-delimited JSON:
//!   * request  - one request, one response, server closes the connection
//!   * subscribe - one `events.subscribe`, then an open-ended event stream
//!
//! Both are verified against herdr 0.7.5 (protocol 17).

use std::cell::Cell;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};

use crate::transport::{self, Stream};

pub type Result<T> = std::result::Result<T, String>;

pub struct Client {
    path: PathBuf,
    seq: Cell<u64>,
}

impl Client {
    /// Resolves the endpoint from an explicit override, then
    /// `HERDR_SOCKET_PATH` (injected into every plugin command), then the
    /// platform default.
    pub fn from_env(override_path: Option<&str>) -> Result<Self> {
        let path = match override_path.filter(|p| !p.is_empty()) {
            Some(p) => PathBuf::from(p),
            None => match std::env::var_os("HERDR_SOCKET_PATH") {
                Some(p) => PathBuf::from(p),
                None => transport::default_socket_path()
                    .ok_or("no home directory, and HERDR_SOCKET_PATH is not set")?,
            },
        };
        if !transport::looks_present(&path) {
            return Err(format!("herdr socket not found at {}", path.display()));
        }
        Ok(Self {
            path,
            seq: Cell::new(0),
        })
    }

    fn connect(&self) -> Result<Stream> {
        let s = Stream::connect(&self.path)
            .map_err(|e| format!("connect {}: {e}", self.path.display()))?;
        s.set_read_timeout(Some(Duration::from_secs(10))).ok();
        s.set_write_timeout(Some(Duration::from_secs(10))).ok();
        Ok(s)
    }

    /// One request/response round trip on its own connection.
    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = {
            let n = self.seq.get() + 1;
            self.seq.set(n);
            format!("tagr:{n}")
        };
        let stream = self.connect()?;
        let mut w = stream.try_clone().map_err(|e| e.to_string())?;
        let body = json!({ "id": id, "method": method, "params": params });
        writeln!(w, "{body}").map_err(|e| format!("{method}: write: {e}"))?;
        w.flush().map_err(|e| format!("{method}: flush: {e}"))?;

        let mut line = String::new();
        BufReader::new(stream)
            .read_line(&mut line)
            .map_err(|e| format!("{method}: read: {e}"))?;
        if line.trim().is_empty() {
            return Err(format!("{method}: empty response"));
        }
        let v: Value = serde_json::from_str(&line).map_err(|e| format!("{method}: parse: {e}"))?;
        if let Some(err) = v.get("error") {
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            let code = err.get("code").and_then(Value::as_str).unwrap_or("error");
            return Err(format!("{method}: {code}: {msg}"));
        }
        v.get("result")
            .cloned()
            .ok_or_else(|| format!("{method}: response had no result"))
    }

    /// Opens the event stream. The returned iterator yields one event per line
    /// and ends when the server closes the connection.
    pub fn subscribe(&self, events: &[&str]) -> Result<Events> {
        let subscriptions: Vec<Value> = events.iter().map(|e| json!({ "type": e })).collect();
        let stream = self.connect()?;
        // The stream is long-lived and mostly idle; a read timeout would abort it.
        stream.set_read_timeout(None).ok();
        let mut w = stream.try_clone().map_err(|e| e.to_string())?;
        let body = json!({
            "id": "tagr:subscribe",
            "method": "events.subscribe",
            "params": { "subscriptions": subscriptions },
        });
        writeln!(w, "{body}").map_err(|e| format!("subscribe: write: {e}"))?;
        w.flush().map_err(|e| format!("subscribe: flush: {e}"))?;

        let mut reader = BufReader::new(stream);
        let mut ack = String::new();
        reader
            .read_line(&mut ack)
            .map_err(|e| format!("subscribe: read ack: {e}"))?;
        let ack_json: Value =
            serde_json::from_str(&ack).map_err(|e| format!("subscribe: parse ack: {e}"))?;
        if let Some(err) = ack_json.get("error") {
            return Err(format!("subscribe rejected: {err}"));
        }
        Ok(Events { reader })
    }
}

pub struct Events {
    reader: BufReader<Stream>,
}

impl Iterator for Events {
    type Item = Value;

    fn next(&mut self) -> Option<Value> {
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) | Err(_) => return None,
                Ok(_) => {}
            }
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                return Some(v);
            }
        }
    }
}
