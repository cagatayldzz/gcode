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
