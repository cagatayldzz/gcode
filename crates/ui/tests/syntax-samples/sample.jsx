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
