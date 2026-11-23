# Performance

Speed is a feature. This page is the toolbox: a benchmark suite for operations, and recipes for responsiveness, CPU, memory and battery.

## Rules of thumb

- **Measure before optimizing**: time the step, then the frames after it.
- Never block the interface thread on slow work.
- Keep dependencies optimized in the dev profile (`opt-level = 3`); the highlighter is ~10× slower without it.
- Compare runs on the **same Mac**, same power source, battery mode, window size, theme and workload; close other apps and let the Mac cool.
  Record the commit, macOS version and machine model with the results.

## Benchmark suite

```sh
cargo bench --locked -p ws-ui --bench performance | tee /tmp/baseline.csv
GCODE_BENCH_SAMPLES=50 cargo bench --locked -p ws-ui --bench performance -- minimap/
cargo test --locked -p ws-ui --bench performance -- --verify
```

The benchmark reports **median and 95th-percentile wall time in microseconds**. It warms each case up three times and runs 20 samples by
default (`GCODE_BENCH_SAMPLES`). It uses a generated project and local Git history; fixture creation and resetting files between samples stay
outside the timed sections.

| Group | Covers |
| --- | --- |
| folder tree | loading the Explorer rows |
| search | literal and regex search, replacement planning and writing |
| minimap | sampling |
| Git | summary, file history and diff |
| files | copy and rename |
| preferences | load/save |

`--verify` exercises every case once without recording timings. Regular project checks **compile** the suite without running timings.

## Editor responsiveness, CPU, memory and battery

Generate a scratch fixture; the command prints the directory containing `project` and `other-project`:

```sh
cargo bench --locked -p ws-ui --bench performance -- --fixture
```

Run an optimized gcode build with a scratch `GCODE_DATA_DIR`, add both fixture projects, find gcode's PID in Activity Monitor and record one
trace per scenario. Keep the CSVs and Instruments traces **outside the repository**.

```sh
bash scripts/profile-app.sh PID 90 > /tmp/idle.csv
```

The sampler records the CPU percentage reported by `ps` and resident memory in KiB once per second. Note initial, peak and settled memory, and
whether it grows after repeating the same work. Check Energy in Activity Monitor as a separate, relative comparison.

| Scenario | What to measure |
| --- | --- |
| Idle with Explorer visible, then minimized | 90 s in each state; periodic CPU spikes; Energy Impact |
| Explorer and file opening | expand/collapse, hover between folders, open and reopen the first file; CPU while scrolling; does the revealed file appear immediately |
| Editor and minimap | open `large.rs`, type steadily, scroll, drag the minimap top to bottom; first open vs cached reopen; stalls; peak memory |
| Tabs and settings | alternate preview and pinned tabs, bulk close, switch projects, change the Explorer font size repeatedly; delayed redraws, memory growth |
| Search and replace | search `benchmark_marker`, then regex and a folder filter; change queries quickly (cancellation); time to results, peak memory |
| Git and file commands | open Changes and Timeline for `timeline.rs`; reveal, copy, rename and trash a disposable file |
| Terminal | a repeatable output burst; CPU and Energy while output flows and after idle |

Use Instruments **Time Profiler** for CPU spikes and **Allocations** for retained memory. For input latency, mark when the action starts
and inspect the following frames, including those spent waiting for the first visible update.

## A short history of what made things fast

| Problem | Fix |
| --- | --- |
| First open of a file stalled while the highlighter compiled | hover prefetch, language warm-up, editor cache |
| A repository with thousands of changed files made the Changes list crawl | virtualized rows with a fixed height |
| Project switching | each project's views, tabs and shells are put away and brought back, never rebuilt |
| Go to File over big projects | a saved index, validated with one `stat` per folder, and ranking on a background thread |

See [Editor & performance](../architecture/editor.md).
