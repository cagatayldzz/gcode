use crate::{file_path, file_uri, Location, Problem, Severity};
use futures::channel::{
  mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
  oneshot,
};
use serde_json::{json, Value};
use std::{
  collections::HashMap,
  io::{self, BufRead, BufReader, Read, Write},
  path::{Path, PathBuf},
  process::{Child, ChildStdin, Command, Stdio},
  sync::{Arc, Mutex},
};

/// The problems of one file, as the server last reported them. An empty list means the file is clean.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
  pub path: PathBuf,
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

#[derive(Default)]
struct State {
  /// The server answered `initialize`; until then messages wait in `queue`.
  ready: bool,
  queue: Vec<Value>,
  versions: HashMap<PathBuf, i64>,
  /// Requests for problems that are not answered yet: the file and the version asked about.
  pending: HashMap<i64, (PathBuf, i64)>,
  /// Requests for where a name is defined that are not answered yet.
  definitions: HashMap<i64, oneshot::Sender<Vec<Location>>>,
  next_id: i64,
}

struct Shared {
  stdin: Mutex<ChildStdin>,
  state: Mutex<State>,
}

impl Shared {
  fn write(&self, message: &Value) {
    let body = message.to_string();
    if let Ok(mut stdin) = self.stdin.lock() {
      // A server that has gone away is noticed by the reader; a failed write needs no report of its own.
      let _ = write!(stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).and_then(|()| stdin.flush());
    }
  }

  /// Sends a message now, or when the server is ready.
  fn send(&self, message: Value) {
    let Ok(mut state) = self.state.lock() else { return };
    if state.ready {
      drop(state);
      self.write(&message);
    } else {
      state.queue.push(message);
    }
  }
}

/// A running language server for one project.
pub struct Session {
  shared: Arc<Shared>,
  child: Mutex<Child>,
}

impl Session {
  /// Starts `server` in its project folder. Problems arrive from the returned channel; it ends when the server does.
  pub fn start(server: &Server) -> io::Result<(Session, UnboundedReceiver<Update>)> {
    let mut child = Command::new(&server.program)
      .args(&server.args)
      .current_dir(&server.root)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::null())
      .spawn()?;
    let missing = || io::Error::new(io::ErrorKind::BrokenPipe, "the language server has no pipes");
    let stdin = child.stdin.take().ok_or_else(missing)?;
    let stdout = child.stdout.take().ok_or_else(missing)?;
    let shared = Arc::new(Shared { stdin: Mutex::new(stdin), state: Mutex::new(State::default()) });
    let (sender, receiver) = unbounded();
    let reader = shared.clone();
    std::thread::Builder::new().name("language-server".into()).spawn(move || read_messages(BufReader::new(stdout), &reader, &sender))?;
    shared.write(&json!({
      "jsonrpc": "2.0",
      "id": 0,
      "method": "initialize",
      "params": {
        "processId": std::process::id(),
        "rootUri": file_uri(&server.root),
        "capabilities": {
          "textDocument": {
            "synchronization": { "didSave": false },
            "publishDiagnostics": { "relatedInformation": false },
            "diagnostic": { "dynamicRegistration": false },
          },
          "workspace": { "configuration": true },
        },
        "workspaceFolders": [{ "uri": file_uri(&server.root), "name": "project" }],
      },
    }));
    Ok((Session { shared, child: Mutex::new(child) }, receiver))
  }

  /// Tells the server the text of a file (opening it the first time) and asks for its problems.
  pub fn sync(&self, path: &Path, text: &str) {
    let Some(language) = language_id(path) else { return };
    let uri = file_uri(path);
    let (version, id, opened) = {
      let Ok(mut state) = self.shared.state.lock() else { return };
      let opened = !state.versions.contains_key(path);
      let version = state.versions.get(path).copied().unwrap_or(0) + 1;
      state.versions.insert(path.to_path_buf(), version);
      state.next_id += 1;
      let id = state.next_id;
      state.pending.insert(id, (path.to_path_buf(), version));
      (version, id, opened)
    };
    if opened {
      self.shared.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": uri, "languageId": language, "version": version, "text": text } },
      }));
    } else {
      self.shared.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": { "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] },
      }));
    }
    self.shared.send(json!({
      "jsonrpc": "2.0",
      "id": id,
      "method": "textDocument/diagnostic",
      "params": { "textDocument": { "uri": uri } },
    }));
  }

  /// Asks where the name at a position is defined. The answer is empty when there is none or the server fails.
  pub fn definition(&self, path: &Path, line: u32, column: u32) -> oneshot::Receiver<Vec<Location>> {
    let (sender, receiver) = oneshot::channel();
    let id = {
      let Ok(mut state) = self.shared.state.lock() else { return receiver };
      state.next_id += 1;
      let id = state.next_id;
      state.definitions.insert(id, sender);
      id
    };
    self.shared.send(json!({
      "jsonrpc": "2.0",
      "id": id,
      "method": "textDocument/definition",
      "params": { "textDocument": { "uri": file_uri(path) }, "position": { "line": line, "character": column } },
    }));
    receiver
  }

  /// Tells the server a file is no longer open.
  pub fn close(&self, path: &Path) {
    let was_open = self.shared.state.lock().is_ok_and(|mut state| state.versions.remove(path).is_some());
    if was_open {
      self.shared.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didClose",
        "params": { "textDocument": { "uri": file_uri(path) } },
      }));
    }
  }
}

impl Drop for Session {
  fn drop(&mut self) {
    if let Ok(mut child) = self.child.lock() {
      let _ = child.kill();
      let _ = child.wait();
    }
  }
}

fn read_messages(mut input: BufReader<impl Read>, shared: &Shared, updates: &UnboundedSender<Update>) {
  loop {
    let mut length = None;
    loop {
      let mut line = String::new();
      match input.read_line(&mut line) {
        Ok(0) | Err(_) => return,
        Ok(_) => {}
      }
      let line = line.trim();
      if line.is_empty() {
        break;
      }
      if let Some((name, value)) = line.split_once(':') {
        if name.eq_ignore_ascii_case("content-length") {
          length = value.trim().parse::<usize>().ok();
        }
      }
    }
    let Some(length) = length else { continue };
    let mut body = vec![0; length];
    if input.read_exact(&mut body).is_err() {
      return;
    }
    let Ok(message) = serde_json::from_slice::<Value>(&body) else { continue };
    handle(&message, shared, updates);
  }
}

fn handle(message: &Value, shared: &Shared, updates: &UnboundedSender<Update>) {
  let method = message.get("method").and_then(Value::as_str);
  let id = message.get("id");
  match (method, id) {
    // A request from the server (registering a capability, asking for settings): nothing here needs a real answer.
    (Some(method), Some(id)) => {
      let result = if method == "workspace/configuration" {
        let count = message.pointer("/params/items").and_then(Value::as_array).map_or(0, Vec::len);
        Value::Array(vec![Value::Null; count])
      } else {
        Value::Null
      };
      shared.write(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }
    (Some("textDocument/publishDiagnostics"), None) => {
      let params = &message["params"];
      let Some(path) = params["uri"].as_str().and_then(file_path) else { return };
      let problems = params["diagnostics"].as_array().map(|items| items.iter().filter_map(problem).collect()).unwrap_or_default();
      let _ = updates.unbounded_send(Update { path, problems });
    }
    (Some(_), None) => {}
    (None, Some(id)) => {
      let Some(id) = id.as_i64() else { return };
      if id == 0 {
        shared.write(&json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
        let queued = shared.state.lock().map(|mut state| {
          state.ready = true;
          std::mem::take(&mut state.queue)
        });
        for message in queued.unwrap_or_default() {
          shared.write(&message);
        }
        return;
      }
      let answered = shared.state.lock().ok().and_then(|mut state| state.definitions.remove(&id));
      if let Some(sender) = answered {
        let _ = sender.send(parse_locations(&message["result"]));
        return;
      }
      let Some((path, version)) = shared.state.lock().ok().and_then(|mut state| {
        let asked = state.pending.remove(&id)?;
        // An answer about text that has changed since is out of date; the newer request will answer.
        (state.versions.get(&asked.0) == Some(&asked.1)).then_some(asked)
      }) else {
        return;
      };
      let _ = version;
      let result = &message["result"];
      if result["kind"].as_str() == Some("full") {
        let problems = result["items"].as_array().map(|items| items.iter().filter_map(problem).collect()).unwrap_or_default();
        let _ = updates.unbounded_send(Update { path, problems });
      }
    }
    (None, None) => {}
  }
}

fn problem(item: &Value) -> Option<Problem> {
  let number = |pointer: &str| item.pointer(pointer).and_then(Value::as_u64).map(|value| value as u32);
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
      let number = |pointer: &str| item.pointer(pointer).and_then(Value::as_u64).map(|value| value as u32);
      let (uri, range) = if item.get("targetUri").is_some() { ("/targetUri", "/targetSelectionRange") } else { ("/uri", "/range") };
      let origin = (|| {
        let from = |pointer: &str| item.pointer(pointer).and_then(Value::as_u64).map(|value| value as u32);
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
