# Syntax highlighting samples

To check language detection and token colors in the editor, save a code block under the filename shown above it.

## `sample.css`

```css
/* Syntax sample: CSS. */
@charset "utf-8";
@import url("reset.css") layer(base);
@font-face {
  font-family: "Geist";
  src: url("geist.woff2") format("woff2");
}

:root {
  --accent: #7ee787;
  --space: 4px;
  --font: "Geist", system-ui, sans-serif;
}

html,
body {
  margin: 0;
  font: 14px/1.5 var(--font);
  color: rgb(240 246 252 / 80%);
  background: linear-gradient(to right, #0d1117 0%, hsl(215deg 28% 17%) 100%);
}

.button.primary:hover > span::before,
#main a[href^="https"]:not(.plain):nth-child(2n + 1) {
  padding: calc(var(--space) * 2) 1.5rem;
  border: 1px solid var(--accent) !important;
  transition: color 0.2s ease-in-out, transform 150ms;
  content: "\201C";
}

@media (min-width: 768px) and (prefers-color-scheme: dark) {
  .card {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  }
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

@supports (display: grid) {
  .grid { display: grid; }
}
```

## `sample.go`

```go
// Syntax sample: Go.
package main

import (
	"errors"
	"fmt"
	"os"
	str "strings"
)

const Limit = 16

var ErrMissing = errors.New("missing")

type Role int

const (
	Admin Role = iota
	Guest
)

type User struct {
	ID    int    `json:"id"`
	Name  string `json:"name"`
	Roles []Role
}

type Greeter interface {
	Greet(prefix string) string
}

func (u *User) Greet(prefix string) string {
	return fmt.Sprintf("%s, %s!", prefix, u.Name)
}

func find[T any](items []T, keep func(T) bool) (T, error) {
	var zero T
	for _, item := range items {
		if keep(item) {
			return item, nil
		}
	}
	return zero, ErrMissing
}

/* A block
   comment. */
func main() {
	users := []User{{ID: 1, Name: "ada"}, {ID: 2, Name: "bob"}}
	lookup := map[string]*User{}
	ch := make(chan int, 2)
	raw := `raw
string`
	r := 'x'
	go func() { ch <- 1 }()
	defer close(ch)
	for i, user := range users {
		lookup[str.ToUpper(user.Name)] = &users[i]
		switch {
		case user.ID > 1:
			fmt.Println(user.Greet("Hello"), raw, r, 0xFF, 3.5e2)
		default:
			continue
		}
	}
	if _, err := find(users, func(u User) bool { return u.ID == 3 }); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
```

## `sample.html`

```html
<!DOCTYPE html>
<!-- Syntax sample: HTML. -->
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Sample &amp; Test</title>
    <link rel="stylesheet" href="style.css" />
    <style>
      body { color: #fff; margin: 0; }
      .box > p:first-child { font-weight: bold; }
    </style>
    <script type="module">
      const answer = 42;
      document.querySelector("#main").addEventListener("click", () => console.log(answer));
    </script>
  </head>
  <body class="page dark" data-theme='dark' hidden>
    <header id="top">
      <nav aria-label="Main">
        <a href="https://example.com/?a=1&b=2" target="_blank" rel="noopener">Home</a>
        <a href="#about">About</a>
      </nav>
    </header>
    <main id="main">
      <h1>Hello, <em>world</em>&nbsp;&copy;</h1>
      <img src="logo.png" alt="Logo" width="32" height="32" />
      <form action="/submit" method="post">
        <label for="name">Name</label>
        <input id="name" name="name" type="text" required disabled />
        <select><option value="1" selected>One</option></select>
        <button type="submit" onclick="send()">Send</button>
      </form>
      <table>
        <tr><th>Name</th><td>Ada</td></tr>
      </table>
      <pre><code>let x = 1 &lt; 2;</code></pre>
    </main>
  </body>
</html>
```

## `sample.js`

```javascript
// Syntax sample: JavaScript.
"use strict";

const path = require("path");
const { EventEmitter } = require("events");
import defaultExport, { named as alias } from "./module.js";

/**
 * A documented class.
 * @param {number} start
 */
class Counter extends EventEmitter {
  static instances = 0;
  #count = 0;

  constructor(start = 0) {
    super();
    this.#count = start;
    Counter.instances++;
  }

  get value() {
    return this.#count;
  }

  increment(step = 1) {
    this.#count += step;
    this.emit("change", this.#count);
    return this;
  }
}

const pattern = /(\d+)-(?<name>[a-z]+)/gi;
const template = `Total: ${[1, 2, 3].reduce((a, b) => a + b, 0)} for ${path.sep}`;
const object = { key: "value", "quoted-key": 1, [`computed`]: true, method() {}, get x() { return 1; } };
const { key, ...rest } = object;
const big = 10n ** 3n;

function* generate(limit) {
  for (let i = 0; i < limit; i++) yield i;
}

async function main() {
  try {
    const response = await fetch("https://example.com/api", { method: "POST" });
    const data = await response.json();
    console.log(data?.items?.[0] ?? null, key, rest, big, pattern.exec("1-a"));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
  } finally {
    new Counter(1).increment().on("change", (value) => void value);
  }
  switch (typeof defaultExport) {
    case "function":
      break;
    default:
      alias();
  }
}

main().then(() => process.exit(0));
```

## `sample.jsx`

```jsx
// Syntax sample: JSX.
import { useState } from "react";

const items = ["one", "two"];

export function Counter({ start = 0, label }) {
  const [value, setValue] = useState(start);

  return (
    <section className="counter" data-value={value} hidden={false}>
      <h1 id="title">{label ?? "Counter"}</h1>
      <p>
        Count: <strong>{value}</strong> &nbsp;&copy;
      </p>
      <button type="button" onClick={() => setValue((current) => current + 1)}>
        Add
      </button>
      <ul>
        {items.map((item) => (
          <li key={item} style={{ color: "red" }}>
            {item}
          </li>
        ))}
      </ul>
      {/* A JSX comment. */}
      {value > 3 ? <em>Many</em> : <Fallback reason="few" {...{ extra: true }} />}
      <>
        <input value={value} readOnly />
      </>
    </section>
  );
}

function Fallback({ reason }) {
  return <i>{reason}</i>;
}
```

## `sample.kt`

```kotlin
// Syntax sample: Kotlin.
package sample.app

import kotlin.math.max
import kotlinx.coroutines.*

/** A documented class. */
@Suppress("unused")
data class User(val id: Int, var name: String = "guest", val tags: List<String> = emptyList())

sealed interface Result<out T> {
    data class Ok<T>(val value: T) : Result<T>
    data class Failure(val error: Throwable) : Result<Nothing>
}

enum class Level(val weight: Int) { LOW(1), HIGH(10) }

object Registry {
    private val users = mutableMapOf<Int, User>()
    const val LIMIT = 16

    fun add(user: User): Boolean = users.putIfAbsent(user.id, user) == null
}

abstract class Base<T : Comparable<T>> {
    abstract fun compare(a: T, b: T): Int
    open fun describe(): String = "base"
}

fun User.greet(prefix: String = "Hello"): String = "$prefix, ${name.uppercase()}! ($id)"

inline fun <reified T> List<*>.firstOf(): T? = this.firstOrNull { it is T } as? T

suspend fun load(id: Int): Result<User> = withContext(Dispatchers.IO) {
    try {
        delay(100L)
        Result.Ok(User(id, "loaded"))
    } catch (error: Exception) {
        Result.Failure(error)
    }
}

fun main() = runBlocking {
    val raw = """
        raw "string" with $LIMIT
    """.trimIndent()
    val char = 'x'
    val number = 0xFF + 1_000L + 3.5f
    val users = listOf(User(1, "ada"), User(2, "bob", listOf("admin")))
    for ((index, user) in users.withIndex()) {
        when {
            user.id > 1 && user.tags.isNotEmpty() -> println(user.greet())
            index == 0 -> continue
            else -> break
        }
    }
    val result = load(max(1, 2))
    if (result is Result.Ok) println(result.value.name ?: "none") else println(raw + char + number)
    users.map { it.name }.filter { it.length > 2 }.forEach(::println)
}
```

## `sample.py`

```python
#!/usr/bin/env python3
"""Syntax sample: Python (module docstring)."""

import asyncio
import os.path as osp
from dataclasses import dataclass, field
from typing import Callable, Generic, Optional, TypeVar

T = TypeVar("T")
LIMIT: int = 0x10
PI = 3.14e-2
NAME = r"raw\string"


@dataclass(frozen=True)
class User:
    """A user with a name."""

    id: int
    name: str = "guest"
    tags: list[str] = field(default_factory=list)

    def greet(self, other: Optional["User"] = None) -> str:
        label = f"Hello, {self.name}! {other.name if other else ''}"
        return label.upper()

    @property
    def initials(self) -> str:
        return "".join(part[0] for part in self.name.split())

    @staticmethod
    def parse(raw: bytes) -> "User":
        return User(id=int(raw), name=raw.decode("utf-8"))


class Box(Generic[T]):
    def __init__(self, value: T) -> None:
        self.value = value

    def map(self, fn: Callable[[T], T]) -> "Box[T]":
        return Box(fn(self.value))


async def fetch(url: str, *args, **kwargs) -> dict:
    await asyncio.sleep(0.1)
    return {"url": url, "ok": True, "items": [1, 2, 3], "none": None}


def main() -> None:
    users = [User(1, "ada"), User(2, "bob")]
    squares = {n: n ** 2 for n in range(5) if n % 2 == 0}
    try:
        for user in users:
            if user.id > 1 and not user.tags:
                print(user.greet(), squares)
                continue
            elif user.name is None:
                break
    except (ValueError, KeyError) as error:
        raise RuntimeError("failed") from error
    finally:
        lambda x, y=2: x + y
    with open(osp.join(os.sep, "tmp"), "rb") as handle:
        assert handle, "no handle"


if __name__ == "__main__":
    main()
```

## `sample.swift`

```swift
// Syntax sample: Swift.
import Foundation
import SwiftUI

/// A documented protocol.
protocol Describable {
  var summary: String { get }
  func describe(prefix: String) -> String
}

enum Level: Int, CaseIterable {
  case low = 1
  case high
  indirect case nested(Level)
}

struct User: Identifiable, Codable, Describable {
  let id: Int
  var name: String
  var tags: [String] = []
  private(set) var level: Level = .low

  var summary: String { "\(name) #\(id)" }

  func describe(prefix: String = "Hello") -> String {
    "\(prefix), \(name)!"
  }

  mutating func rename(to newName: String) {
    name = newName
  }
}

final class Store<Element: Hashable>: ObservableObject {
  @Published private(set) var items: Set<Element> = []
  static let shared = Store<Int>()
  weak var delegate: AnyObject?

  init() {}

  func add(_ item: Element) -> Bool {
    items.insert(item).inserted
  }
}

extension Array where Element == User {
  func names() -> [String] { map(\.name) }
}

actor Counter {
  private var value = 0
  func increment() -> Int { value += 1; return value }
}

struct ContentView: View {
  @State private var count = 0
  @Environment(\.colorScheme) var scheme

  var body: some View {
    VStack(spacing: 8) {
      Text("Count: \(count)").font(.headline)
      Button("Add") { count += 1 }
    }
  }
}

let raw = #"raw "string" \#(1 + 1)"#
let multi = """
  multi-line \(raw)
  """
let numbers = [1, 0xFF, 0b101, 3.5e2, 1_000]
let user: User? = User(id: 1, name: "ada")

if let user, user.id > 0, !user.tags.isEmpty {
  print(user.describe())
} else {
  guard let first = numbers.first else { fatalError("empty") }
  switch first {
  case 1...3: print("low")
  case let value where value > 10: print(value)
  default: break
  }
}

Task { await Counter().increment() }
do {
  try JSONEncoder().encode(user)
} catch {
  print(error)
}
```

## `sample.ts`

```typescript
// Syntax sample: TypeScript.
import type { Readable } from "node:stream";
import { readFile as read, type PathLike } from "node:fs/promises";
import * as path from "node:path";

/** A documented enum. */
export enum Level {
  Low = 1,
  High = "high",
}

export const enum Flag {
  On,
  Off,
}

declare const VERSION: string;
declare module "virtual" {
  export const answer: number;
}

export interface User {
  readonly id: number;
  name: string;
  tags?: string[];
  [key: string]: unknown;
}

export type Result<T, E extends Error = Error> = { ok: true; value: T } | { ok: false; error: E };
type Keys = keyof User;
type Mapped = { [K in Keys]?: User[K] };
type Cond<T> = T extends string ? "text" : T extends number ? "number" : never;

abstract class Store<T extends { id: number }> {
  private items = new Map<number, T>();
  protected static count = 0;
  #secret = Symbol("secret");

  constructor(public readonly name: string, private limit: number = 10) {}

  abstract validate(item: T): boolean;

  add(item: T): this {
    if (!this.validate(item) || this.items.size >= this.limit) {
      throw new RangeError(`Store ${this.name} is full`);
    }
    this.items.set(item.id, item);
    return this;
  }

  get size(): number {
    return this.items.size;
  }
}

function assertUser(value: unknown): asserts value is User {
  if (typeof value !== "object" || value === null) throw new TypeError("not a user");
}

export async function load<T>(file: PathLike, parse: (text: string) => T): Promise<Result<T>> {
  try {
    const text = await read(file, { encoding: "utf8" });
    return { ok: true, value: parse(text) };
  } catch (error) {
    return { ok: false, error: error as Error };
  }
}

const pattern = /^[a-z]+\d{2,}$/giu;
const big = 123_456n;
const hex = 0xff;
const template = `Level ${Level.High} of ${path.basename("a/b.ts")}`;
const optional = (null as User | null)?.name ?? "anonymous";
let tuple: [string, number, ...boolean[]] = ["a", 1, true];
const fn = <T,>(value: T): T => value;

@sealed
class Decorated {
  @log method(@inject() arg: string) {}
}

label: for (const [index, char] of [..."abc"].entries()) {
  if (index > 1) break label;
  console.log(char, VERSION, pattern.test(char), big, hex, template, optional, tuple, fn(1));
}
```

## `sample.tsx`

```tsx
// Syntax sample: TSX.
import React, { useEffect, useState, type ReactNode } from "react";
import { Icons } from "./icons";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  count?: number;
  icon?: ReactNode;
  onPress?: (event: React.MouseEvent<HTMLButtonElement>) => void;
}

type ListProps<T> = { items: readonly T[]; render: (item: T, index: number) => ReactNode };

function List<T extends { id: string }>({ items, render }: ListProps<T>) {
  return (
    <ul className="list">
      {items.map((item, index) => (
        <li key={item.id}>{render(item, index)}</li>
      ))}
    </ul>
  );
}

export function Button({ label, count = 0, icon, onPress, ...rest }: ButtonProps) {
  const [pressed, setPressed] = useState<boolean>(false);

  useEffect(() => {
    document.title = `${label} (${count})`;
    return () => void (document.title = "");
  }, [label, count]);

  return (
    <>
      <button
        type="button"
        className={pressed ? "btn btn-active" : "btn"}
        aria-pressed={pressed}
        data-count={count}
        onClick={(event) => {
          setPressed((value) => !value);
          onPress?.(event);
        }}
        {...rest}
      >
        {icon ?? <Icons.Plus size={16} />}
        <span style={{ marginLeft: 4 }}>{label}</span>
        {count > 0 && <b>{count}</b>}
      </button>
      {/* A JSX comment. */}
      <List items={[{ id: "a" }, { id: "b" }]} render={(item) => <em>{item.id}</em>} />
    </>
  );
}

export default function App(): JSX.Element {
  return <Button label="Save &amp; close" count={3} disabled />;
}
```

## `Dockerfile`

```dockerfile
# syntax=docker/dockerfile:1
# Syntax sample: Dockerfile.
ARG NODE_VERSION=22
FROM --platform=linux/amd64 node:${NODE_VERSION}-alpine AS build
LABEL org.opencontainers.image.title="sample" maintainer='someone'
ENV NODE_ENV=production \
    PORT=3000
WORKDIR /app
COPY --chown=node:node package.json bun.lock ./
RUN --mount=type=cache,target=/root/.cache npm ci && \
    npm run build
COPY . .

FROM nginx:stable AS runtime
EXPOSE 3000/tcp
USER node
VOLUME ["/data"]
HEALTHCHECK --interval=30s CMD curl -f http://localhost:$PORT/ || exit 1
ONBUILD RUN echo "child build"
COPY --from=build /app/dist /usr/share/nginx/html
ENTRYPOINT ["nginx", "-g", "daemon off;"]
CMD ["--help"]
```
