# Syntax sample: Python.
from dataclasses import dataclass


@dataclass
class User:
    id: int
    name: str = "guest"

    def greet(self) -> str:
        return f"Hello, {self.name}!"


def main() -> None:
    users = [User(1, "ada"), User(2)]
    for user in users:
        if user.id > 1 and user.name:
            print(user.greet())
        else:
            print("skipped", None, True)


if __name__ == "__main__":
    main()
