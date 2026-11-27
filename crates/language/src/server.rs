use crate::{file_path, file_uri, Location, Problem, Severity};
use futures::channel::{
  mpsc::{channel, Receiver, Sender},
  oneshot,
};
use serde_json::{json, Value};
use std::{
  collections::HashMap,
  io::{self, BufRead, BufReader, Read, Write},
  path::{Path, PathBuf},
  process::{Child, Command, Stdio},
  sync::{mpsc, Arc, Mutex},
  time::{Duration, Instant},
};

/// The problems of one file, as the server last reported them. An empty list means the file is clean.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
  pub path: PathBuf,
  pub version: i64,
  pub problems: Vec<Problem>,
}

/// How to start a language server: a program, its arguments and the project folder it serves. Files that share a root
/// share one server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
  pub program: PathBuf,
  pub args: Vec<String>,
  pub root: PathBuf,
}

/// The language the protocol calls a file, or `None` for files no server here handles (JavaScript and TypeScript).
pub fn language_id(path: &Path) -> Option<&'static str> {
  match path.extension()?.to_str()? {
    "ts" | "mts" | "cts" => Some("typescript"),
    "tsx" => Some("typescriptreact"),
    "js" | "mjs" | "cjs" => Some("javascript"),
    "jsx" => Some("javascriptreact"),
    _ => None,
  }
}

/// Looks for the project's TypeScript server, then an installed server on PATH. Both can load project code, so
/// discovery requires the canonical workspace root approved by the user and a file inside it.
pub fn find_server(file: &Path, trusted_root: Option<&Path>) -> Option<Server> {
  language_id(file)?;
  let root = trusted_root?;
  let file = file.canonicalize().ok()?;
  let parent = file.parent().filter(|parent| parent.starts_with(root))?;
  let mut ancestors = parent.ancestors().take_while(|folder| folder.starts_with(root));
  let platform = format!(
    "typescript-{}-{}",
    match std::env::consts::OS {
      "macos" => "darwin",
      "windows" => "win32",
      other => other,
    },
    match std::env::consts::ARCH {
      "aarch64" => "arm64",
      "x86_64" => "x64",
      other => other,
    }
  );
  let executable = |name: &str| if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
  for folder in ancestors.clone() {
    let native = folder.join("node_modules/@typescript").join(&platform).join("lib").join(executable("tsc"));
    if native.is_file() {
      return Some(Server { program: native, args: vec!["--lsp".into(), "--stdio".into()], root: folder.to_path_buf() });
    }
    let local = folder.join("node_modules/.bin").join(if cfg!(windows) { "typescript-language-server.cmd" } else { "typescript-language-server" });
    if local.is_file() {
      return Some(Server { program: local, args: vec!["--stdio".into()], root: folder.to_path_buf() });
    }
  }
  let fallback_root =
    ancestors.find(|folder| ["tsconfig.json", "package.json", ".git"].iter().any(|marker| folder.join(marker).exists())).unwrap_or(parent);
  let mut folders: Vec<PathBuf> = std::env::var_os("PATH").map(|path| std::env::split_paths(&path).collect()).unwrap_or_default();
  folders.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
  folders
    .into_iter()
    .filter(|folder| folder.is_absolute())
    .map(|folder| folder.join("typescript-language-server"))
    .find(|candidate| candidate.is_file())
    .map(|program| Server { program, args: vec!["--stdio".into()], root: fallback_root.to_path_buf() })
}

const MAX_HEADER: u64 = 8 * 1024;
const MAX_BODY: usize = 16 * 1024 * 1024;
const MAX_QUEUED: usize = 128;
const MAX_PENDING: usize = 64;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(12);

struct Document {
  version: i64,
  text: String,
}

enum Reply {
  Request(oneshot::Sender<Result<Value, String>>),
  Diagnostic(PathBuf, i64),
}

struct Pending {
  deadline: Instant,
  reply: Reply,
}

#[derive(Default)]
struct State {
  ready: bool,
  stopped: bool,
  supports_pull_diagnostics: bool,
  incremental_sync: bool,
  formatting: bool,
  documents: HashMap<PathBuf, Document>,
  pending: HashMap<i64, Pending>,
  next_id: i64,
}

impl State {
  fn stop(&mut self) {
    self.stopped = true;
    self.pending.clear();
    self.documents.clear();
  }

  fn request_id(&mut self, reply: Reply) -> Option<i64> {
    if self.stopped || self.pending.len() >= MAX_PENDING {
      return None;
    }
    self.next_id += 1;
    self.pending.insert(self.next_id, Pending { deadline: Instant::now() + REQUEST_TIMEOUT, reply });
    Some(self.next_id)
  }
}

struct Shared {
  outgoing: mpsc::SyncSender<Value>,
  state: Mutex<State>,
}

impl Shared {
  fn stop(&self) {
    if let Ok(mut state) = self.state.lock() {
      state.stop();
    }
  }

  fn write(&self, state: &mut State, message: Value) {
    if !state.stopped && self.outgoing.try_send(message).is_err() {
      state.stop();
    }
  }

  fn diagnostics(&self, state: &mut State, path: &Path, version: i64) {
    if !state.supports_pull_diagnostics {
      return;
    }
    state.pending.retain(|_, pending| !matches!(&pending.reply, Reply::Diagnostic(other, _) if other == path));
    if let Some(id) = state.request_id(Reply::Diagnostic(path.to_path_buf(), version)) {
      self.write(
        state,
        json!({
          "jsonrpc": "2.0", "id": id, "method": "textDocument/diagnostic",
          "params": { "textDocument": { "uri": file_uri(path) } },
        }),
      );
    }
  }
}

/// A running language server for one project.
pub struct Session {
  shared: Arc<Shared>,
  child: Arc<Mutex<Option<Child>>>,
}

impl Session {
  /// Problems arrive until the server exits or the connection is closed.
  pub fn start(server: &Server) -> io::Result<(Session, Receiver<Update>)> {
    let mut child = Command::new(&server.program)
      .args(&server.args)
      .current_dir(&server.root)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::null())
      .spawn()?;
    let missing = || io::Error::new(io::ErrorKind::BrokenPipe, "the language server has no pipes");
    let mut stdin = child.stdin.take().ok_or_else(missing)?;
    let stdout = child.stdout.take().ok_or_else(missing)?;
    let (outgoing, messages) = mpsc::sync_channel(MAX_QUEUED + 1);
    let shared = Arc::new(Shared { outgoing, state: Mutex::new(State::default()) });
    let child = Arc::new(Mutex::new(Some(child)));
    let session = Session { shared: shared.clone(), child: child.clone() };
    let monitor = shared.clone();
    let (mut sender, receiver) = channel(MAX_PENDING);
    let mut watchdog_updates = sender.clone();
    std::thread::Builder::new().name("lsp-watchdog".into()).spawn(move || {
      let started = Instant::now();
      loop {
        {
          let Ok(mut state) = monitor.state.lock() else { break };
          if !state.ready && started.elapsed() >= INITIALIZE_TIMEOUT {
            state.stop();
          }
          let now = Instant::now();
          state.pending.retain(|_, request| request.deadline > now && !matches!(&request.reply, Reply::Request(sender) if sender.is_canceled()));
          if state.stopped {
            break;
          }
        }
        if child.lock().is_ok_and(|mut child| child.as_mut().is_some_and(|child| child.try_wait().is_ok_and(|status| status.is_some()))) {
          monitor.stop();
          break;
        }
        std::thread::sleep(Duration::from_millis(50));
      }
      watchdog_updates.close_channel();
      let process = child.lock().ok().and_then(|mut child| child.take());
      if let Some(mut process) = process {
        let _ = process.kill();
        let _ = process.wait();
      }
    })?;
    let writer = Arc::downgrade(&shared);
    std::thread::Builder::new().name("lsp-writer".into()).spawn(move || {
      for message in messages {
        let body = message.to_string();
        if write!(stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).and_then(|()| stdin.flush()).is_err() {
          if let Some(shared) = writer.upgrade() {
            shared.stop();
          }
          break;
        }
      }
    })?;
    let initialized = json!({
      "jsonrpc": "2.0", "id": 0, "method": "initialize",
      "params": {
        "processId": std::process::id(),
        "rootUri": file_uri(&server.root),
        "capabilities": {
          "general": { "positionEncodings": ["utf-16"] },
          "textDocument": {
            "synchronization": { "didSave": false },
            "publishDiagnostics": { "versionSupport": true },
            "diagnostic": { "dynamicRegistration": false },
            "completion": { "completionItem": { "snippetSupport": false, "documentationFormat": ["markdown", "plaintext"] } },
            "codeAction": { "codeActionLiteralSupport": { "codeActionKind": { "valueSet": ["quickfix"] } } },
            "hover": { "contentFormat": ["markdown", "plaintext"] }
          },
          "workspace": { "configuration": true }
        },
        "workspaceFolders": [{ "uri": file_uri(&server.root), "name": "project" }]
      }
    });
    if let Ok(mut state) = shared.state.lock() {
      shared.write(&mut state, initialized);
    }
    std::thread::Builder::new().name("lsp-reader".into()).spawn(move || {
      read_messages(BufReader::new(stdout), &shared, &mut sender);
      shared.stop();
    })?;
    Ok((session, receiver))
  }

  /// Opens or updates the document before later requests for it are enqueued.
  pub fn sync(&self, path: &Path, text: &str) {
    let Some(language) = language_id(path) else { return };
    let Ok(mut state) = self.shared.state.lock() else { return };
    if state.stopped || state.documents.get(path).is_some_and(|document| document.text == text) {
      return;
    }
    let uri = file_uri(path);
    // A single monotonically increasing counter also prevents late replies after a close and reopen.
    state.next_id += 1;
    let version = state.next_id;
    let previous = state.documents.insert(path.to_path_buf(), Document { version, text: text.to_owned() });
    if !state.ready {
      return;
    }
    let message = if let Some(previous) = previous {
      let mut change = json!({ "text": text });
      if state.incremental_sync {
        let mut lines = previous.text.split('\n');
        let last = lines.next_back().unwrap_or_default().trim_end_matches('\r');
        change["range"] = json!({ "start": { "line": 0, "character": 0 },
          "end": { "line": lines.count(), "character": last.encode_utf16().count() } });
      }
      json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": { "textDocument": { "uri": uri, "version": version }, "contentChanges": [change] },
      })
    } else {
      open_message(path, language, version, text)
    };
    self.shared.write(&mut state, message);
    self.shared.diagnostics(&mut state, path, version);
  }

  /// Sends a request and resolves it or cancels the receiver at its deadline.
  pub fn request(&self, method: &str, params: Value) -> oneshot::Receiver<Result<Value, String>> {
    let (sender, receiver) = oneshot::channel();
    let Ok(mut state) = self.shared.state.lock() else { return receiver };
    if state.ready {
      if let Some(id) = state.request_id(Reply::Request(sender)) {
        self.shared.write(&mut state, json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
      }
    }
    receiver
  }

  pub fn matches_document(&self, path: &Path, version: i64, text: &str) -> bool {
    self
      .shared
      .state
      .lock()
      .is_ok_and(|state| !state.stopped && state.documents.get(path).is_some_and(|document| document.version == version && document.text == text))
  }

  pub fn supports_formatting(&self) -> bool {
    self.shared.state.lock().is_ok_and(|state| state.ready && !state.stopped && state.formatting)
  }

  pub fn is_running(&self) -> bool {
    self.shared.state.lock().is_ok_and(|state| !state.stopped)
  }

  pub fn shutdown(&self) {
    self.shared.stop();
    if let Ok(mut child) = self.child.lock() {
      if let Some(child) = child.as_mut() {
        let _ = child.kill();
      }
    }
  }

  pub fn close(&self, path: &Path) {
    let Ok(mut state) = self.shared.state.lock() else { return };
    if state.documents.remove(path).is_some() && state.ready {
      self.shared.write(
        &mut state,
        json!({
          "jsonrpc": "2.0", "method": "textDocument/didClose",
          "params": { "textDocument": { "uri": file_uri(path) } },
        }),
      );
    }
  }
}

impl Drop for Session {
  fn drop(&mut self) {
    self.shutdown();
  }
}

fn open_message(path: &Path, language: &str, version: i64, text: &str) -> Value {
  json!({
    "jsonrpc": "2.0", "method": "textDocument/didOpen",
    "params": { "textDocument": { "uri": file_uri(path), "languageId": language, "version": version, "text": text } },
  })
}

fn read_messages(mut input: BufReader<impl Read>, shared: &Shared, updates: &mut Sender<Update>) {
  loop {
    let mut length = None;
    let mut budget = MAX_HEADER;
    loop {
      let mut line = String::new();
      let Ok(read) = (&mut input).take(budget).read_line(&mut line) else { return };
      if read == 0 || !line.ends_with('\n') {
        return;
      }
      budget -= read as u64;
      let line = line.trim();
      if line.is_empty() {
        break;
      }
      if let Some((name, value)) = line.split_once(':') {
        if name.eq_ignore_ascii_case("content-length") {
          if length.is_some() {
            return;
          }
          length = value.trim().parse::<usize>().ok();
          if length.is_none_or(|length| length == 0 || length > MAX_BODY) {
            return;
          }
        }
      }
    }
    let Some(length) = length else { return };
    let mut body = vec![0; length];
    if input.read_exact(&mut body).is_err() {
      return;
    }
    let Ok(message) = serde_json::from_slice::<Value>(&body) else { return };
    handle(&message, shared, updates);
  }
}

fn handle(message: &Value, shared: &Shared, updates: &mut Sender<Update>) {
  let Ok(mut state) = shared.state.lock() else { return };
  if state.stopped {
    return;
  }
  let method = message.get("method").and_then(Value::as_str);
  let id = message.get("id");
  match (method, id) {
    (Some(method), Some(id)) => {
      let result = if method == "workspace/configuration" {
        let count = message.pointer("/params/items").and_then(Value::as_array).map_or(0, Vec::len);
        json!({ "result": vec![Value::Null; count] })
      } else if matches!(method, "client/registerCapability" | "client/unregisterCapability" | "window/workDoneProgress/create") {
        json!({ "result": null })
      } else {
        json!({ "error": { "code": -32601, "message": "Method not supported" } })
      };
      let mut response = json!({ "jsonrpc": "2.0", "id": id });
      response.as_object_mut().unwrap().extend(result.as_object().unwrap().clone());
      shared.write(&mut state, response);
    }
    (Some("textDocument/publishDiagnostics"), None) => {
      let params = &message["params"];
      let Some(path) = params["uri"].as_str().and_then(file_path) else { return };
      let Some(document) = state.documents.get(&path) else { return };
      if params["version"].as_i64().is_some_and(|version| version != document.version) {
        return;
      }
      if !report(updates, path, document.version, &params["diagnostics"]) {
        state.stop();
      }
    }
    (None, Some(id)) => {
      let Some(id) = id.as_i64() else { return };
      if id == 0 {
        if state.ready {
          return;
        }
        if !message["result"].is_object()
          || message.pointer("/result/capabilities/positionEncoding").and_then(Value::as_str).is_some_and(|encoding| encoding != "utf-16")
        {
          state.stop();
          return;
        }
        state.supports_pull_diagnostics = message.pointer("/result/capabilities/diagnosticProvider").is_some_and(Value::is_object);
        shared.write(&mut state, json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
        let sync = &message["result"]["capabilities"]["textDocumentSync"];
        let formatting = &message["result"]["capabilities"]["documentFormattingProvider"];
        state.formatting = formatting == &Value::Bool(true) || formatting.is_object();
        state.incremental_sync = sync.as_u64().or_else(|| sync["change"].as_u64()) == Some(2);
        // While holding the same lock used by sync and requests, enqueue didOpen for the latest copy of each file.
        let documents = std::mem::take(&mut state.documents);
        for (path, document) in &documents {
          if let Some(language) = language_id(path) {
            shared.write(&mut state, open_message(path, language, document.version, &document.text));
            shared.diagnostics(&mut state, path, document.version);
          }
        }
        if !state.stopped {
          state.documents = documents;
          state.ready = true;
        }
      } else if let Some(pending) = state.pending.remove(&id).filter(|pending| pending.deadline > Instant::now()) {
        match pending.reply {
          Reply::Request(sender) => {
            let response = match message.get("error") {
              Some(error) => Err(error["message"].as_str().unwrap_or("The language server rejected the request.").to_owned()),
              None => Ok(message["result"].clone()),
            };
            let _ = sender.send(response);
          }
          Reply::Diagnostic(path, version) => {
            if state.documents.get(&path).is_some_and(|document| document.version == version)
              && message["result"]["kind"].as_str() == Some("full")
              && !report(updates, path, version, &message["result"]["items"])
            {
              state.stop();
            }
          }
        }
      }
    }
    _ => {}
  }
}

fn report(updates: &mut Sender<Update>, path: PathBuf, version: i64, diagnostics: &Value) -> bool {
  const MAX_DIAGNOSTICS: usize = 5000;
  let problems = diagnostics.as_array().map(|items| items.iter().take(MAX_DIAGNOSTICS).filter_map(problem).collect()).unwrap_or_default();
  updates.try_send(Update { path, version, problems }).is_ok()
}

fn problem(item: &Value) -> Option<Problem> {
  let number = |pointer: &str| item.pointer(pointer).and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok());
  Some(Problem {
    line: number("/range/start/line")?,
    column: number("/range/start/character")?,
    end_line: number("/range/end/line")?,
    end_column: number("/range/end/character")?,
    severity: match item.get("severity").and_then(Value::as_u64) {
      Some(2) => Severity::Warning,
      Some(3) => Severity::Info,
      Some(4) => Severity::Hint,
      _ => Severity::Error,
    },
    message: item.get("message")?.as_str()?.to_string(),
    code: item.get("code").map(|code| code.as_str().map_or_else(|| code.to_string(), str::to_string)),
    source: item.get("source").and_then(Value::as_str).map(str::to_string),
  })
}

/// The places of a definition answer: one location, a list of them, or a list of links; anything else is no place.
pub fn parse_locations(result: &Value) -> Vec<Location> {
  let items: Vec<&Value> = match result {
    Value::Array(items) => items.iter().collect(),
    Value::Object(_) => vec![result],
    _ => Vec::new(),
  };
  items
    .into_iter()
    .filter_map(|item| {
      let number = |pointer: &str| item.pointer(pointer).and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok());
      let (uri, range) = if item.get("targetUri").is_some() { ("/targetUri", "/targetSelectionRange") } else { ("/uri", "/range") };
      let origin = (|| {
        let from = |pointer: &str| item.pointer(pointer).and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok());
        Some((
          from("/originSelectionRange/start/line")?,
          from("/originSelectionRange/start/character")?,
          from("/originSelectionRange/end/line")?,
          from("/originSelectionRange/end/character")?,
        ))
      })();
      Some(Location {
        path: file_path(item.pointer(uri)?.as_str()?)?,
        line: number(&format!("{range}/start/line"))?,
        column: number(&format!("{range}/start/character"))?,
        end_line: number(&format!("{range}/end/line"))?,
        end_column: number(&format!("{range}/end/character"))?,
        origin,
      })
    })
    .collect()
}
