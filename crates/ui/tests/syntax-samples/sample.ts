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
