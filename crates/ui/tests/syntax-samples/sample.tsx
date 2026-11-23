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
