// Syntax sample: JSX.
import { useState } from "react";

export function Counter({ start = 0 }) {
  const [value, setValue] = useState(start);

  return (
    <div className="counter" data-value={value}>
      <p>Value: {value}</p>
      <button onClick={() => setValue(value + 1)}>Add</button>
    </div>
  );
}
