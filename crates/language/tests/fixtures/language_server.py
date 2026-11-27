"""A deterministic stdio server for the language client tests."""
import json
import sys
import time

def receive():
  size = None
  while True:
    line = sys.stdin.buffer.readline()
    if not line:
      raise SystemExit
    if not line.strip():
      return json.loads(sys.stdin.buffer.read(size))
    if line.lower().startswith(b"content-length:"):
      size = int(line.split(b":", 1)[1])

def send(message):
  body = json.dumps({"jsonrpc": "2.0", **message}).encode()
  sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
  sys.stdout.buffer.flush()

case = sys.argv[1]
init = receive()
assert init["method"] == "initialize"
if case == "oversized":
  sys.stdout.buffer.write(b"Content-Length: 1000000000\r\n\r\n")
  sys.stdout.buffer.flush()
  sys.stdin.buffer.read()
  raise SystemExit
if case == "truncated":
  sys.stdout.buffer.write(b"Content-Length: 200\r\n\r\n{}")
  sys.stdout.buffer.flush()
  raise SystemExit
if case == "malformed":
  sys.stdout.buffer.write(b"Content-Length: 3\r\n\r\n{{{")
  sys.stdout.buffer.flush()
  sys.stdin.buffer.read()
  raise SystemExit

# The client can write snapshots while initialization is pending.
time.sleep(0.1)
send({"id": init["id"], "result": {"capabilities": {"textDocumentSync": 2}}})
assert receive()["method"] == "initialized"
opened = receive()
assert opened["method"] == "textDocument/didOpen"
doc = opened["params"]["textDocument"]
assert doc["text"] == "🦀 workspace"
problem = {
  "severity": 1,
  "message": "Current diagnostic",
  "range": {"start": {"line": 0, "character": 2}, "end": {"line": 0, "character": 4}},
}
def diagnostics(version, items):
  send({"method": "textDocument/publishDiagnostics", "params": {
    "uri": doc["uri"], "version": version, "diagnostics": items,
  }})

diagnostics(doc["version"] - 1, [])
diagnostics(doc["version"], [problem])
if case == "exit":
  sys.stdin.buffer.read(1)
  raise SystemExit

while True:
  message = receive()
  if message["method"] == "textDocument/didChange":
    change = message["params"]["contentChanges"][0]
    assert change["range"]["end"] == {"line": 0, "character": 12}
    assert change["text"] == "🦀 new text"
  if message["method"] == "test/noReply":
    continue
  if message["method"] == "textDocument/hover":
    send({"id": message["id"], "result": {"contents": {"kind": "markdown", "value": "**string**"}}})
  if message["method"] == "textDocument/completion":
    send({"id": message["id"], "result": {"isIncomplete": False, "items": [{"label": "workspace"}]}})
