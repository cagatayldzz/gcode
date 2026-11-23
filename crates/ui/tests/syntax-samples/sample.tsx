// Syntax sample: TSX.
import React, { useState, type ReactNode } from "react";
import * as Icons from "./icons";

interface ButtonProps {
  label: string;
  count?: number;
  onClick: (event: React.MouseEvent<HTMLButtonElement>) => void;
  children?: ReactNode;
}

const DEFAULT_COUNT = 0;

export function Button({ label, count = DEFAULT_COUNT, onClick, children }: ButtonProps) {
  const [pressed, setPressed] = useState<boolean>(false);

  return (
    <button
      className={`btn ${pressed ? "btn-active" : ""}`}
      type="button"
      disabled={count > 10}
      onClick={(event) => {
        setPressed(!pressed);
        onClick(event);
      }}
    >
      <Icons.Plus size={16} />
      <span>{label}</span>
      {count > 0 && <strong>{count}</strong>}
      {children}
    </button>
  );
}

export default function App(): JSX.Element {
  return (
    <>
      <h1 id="title">Hello, world</h1>
      <Button label="Add" onClick={() => console.log("clicked")} />
    </>
  );
}
