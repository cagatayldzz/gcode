// Syntax sample: JavaScript.
const path = require("path");

class Counter {
  #count = 0;

  increment(step = 1) {
    this.#count += step;
    return this.#count;
  }
}

const counter = new Counter();
const total = [1, 2, 3].map((n) => n * 2).reduce((a, b) => a + b, 0);

console.log(`total: ${total}`, counter.increment(), path.join("a", "b"));
