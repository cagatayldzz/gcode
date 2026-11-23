// Syntax sample: TypeScript.
import { readFile } from "node:fs/promises";

export interface User {
  id: number;
  name: string;
  tags?: readonly string[];
}

type Result<T> = { ok: true; value: T } | { ok: false; error: Error };

const MAX_USERS = 100;

export enum Role {
  Admin = "admin",
  Guest = "guest",
}

@sealed
export class Store<T extends User> {
  private items = new Map<number, T>();

  constructor(private readonly limit: number = MAX_USERS) {}

  add(item: T): Result<T> {
    if (this.items.size >= this.limit) {
      return { ok: false, error: new Error(`limit ${this.limit} reached`) };
    }
    this.items.set(item.id, item);
    return { ok: true, value: item };
  }
}

export async function load(path: string): Promise<User[]> {
  const text = await readFile(path, "utf8");
  return JSON.parse(text).filter((user: User) => /^[a-z]+$/i.test(user.name));
}
