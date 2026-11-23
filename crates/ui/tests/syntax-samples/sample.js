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
