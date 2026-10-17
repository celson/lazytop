# lazytop — Design

Date: 2026-10-17
Status: approved in conversation, pending written-spec review

## 1. Goal

`lazytop` is a terminal system monitor written in Rust, in the spirit of
[lazyrclone](../../../../lazyrclone) (bordered panels, status bar with key hints,
keyboard-driven focus, `--demo` mode, `mise` tasks, GitHub releases installable
with `mise use github:celson/lazytop`). Its information density is modelled on the
SiliconScope dashboard: CPU efficiency/performance clusters, GPU / Media / Neural
Engine, memory and memory bandwidth, network and disk, sensors, processes, power
and a local-AI overview.

### Success criteria

- Runs on macOS (Apple Silicon) and Linux (x86_64 / aarch64) **without root**.
- On Apple Silicon shows: E/P CPU usage + MHz, GPU usage + MHz + W, ANE W,
  system W, memory breakdown + pressure + swap, temperatures and fans, network,
  disk, processes. Memory bandwidth is best-effort.
- On Linux shows: per-core / hybrid CPU, memory, network, disk, hwmon sensors,
  processes; GPU via NVML (NVIDIA) or sysfs (AMD) when present; RAPL power when
  readable.
- Any missing metric renders as `—` or hides its panel; the app never crashes
  because a source is unavailable.
- Sampling never blocks keyboard input. Default refresh 1 s, CPU use of lazytop
  itself stays low (target < 2 % of one core at 1 s on an M3 Pro).
- `--demo` runs anywhere with synthetic data.

### Out of scope (v1)

- Record / replay of metrics.
- Remote machines / fleet view.
- Windows.
- Process actions other than SIGTERM / SIGKILL.

## 2. Stack

| Concern | Crate |
|---|---|
| TUI | `ratatui`, `crossterm` |
| Common metrics | `sysinfo` |
| CLI | `clap` (derive) |
| Config | `serde`, `toml`, `dirs` |
| Errors | `anyhow` |
| Unix syscalls / signals | `libc` |
| macOS FFI | `core-foundation`, `io-kit-sys` (+ hand-written `extern` for IOReport) |
| Linux NVIDIA | `nvml-wrapper` (loads `libnvidia-ml` at runtime) |

Toolchain is pinned in `mise.toml` (`rust = "<stable>"`). mise tasks mirror
lazyrclone: `build`, `run`, `demo`, `test`, `lint`, `install`, `clean`.

## 3. Architecture

```
collector thread ──Snapshot (mpsc)──▶ UI thread (event loop: input + render)
                                          │
                                          └─ History (ring buffers, ~300 samples)
```

- A collector thread calls `Collector::sample()` every `interval`, builds an
  immutable `Snapshot` and sends it over a channel. Interval changes and pause
  are sent back over a control channel.
- The UI thread polls crossterm events with a short timeout, drains the
  snapshot channel, pushes series into `History`, and renders. The UI is
  OS-agnostic: it only reads `Snapshot`.
- Every platform-specific field in `Snapshot` is `Option<_>`.

### Module layout

```
src/
├── main.rs              CLI (--interval, --demo, --config, --log, --version), wiring
├── config.rs            TOML config (~/.config/lazytop/config.toml)
├── model.rs             Snapshot and plain data structs
├── history.rs           ring buffers per series
├── collector/
│   ├── mod.rs           trait Collector, thread spawn, control channel
│   ├── common.rs        sysinfo: processes, networks, disks, base memory, uptime
│   ├── ai.rs            AI runtime detection + model budget
│   ├── demo.rs          synthetic deterministic data (seeded)
│   ├── macos/{ioreport,smc,vm,soc,mod}.rs
│   └── linux/{cpu,hwmon,power,gpu,mod}.rs
└── ui/
    ├── app.rs           state, event loop, focus/zoom, modes
    ├── theme.rs         palettes and overrides
    ├── widgets/{bar,chart,panel}.rs
    └── panels/{header,ai,cpu,gpu,memory,netdisk,sensors,processes}.rs
```

### Core types (sketch)

```rust
pub struct Snapshot {
    pub at: Instant,
    pub host: HostInfo,                 // chip/cpu name, core counts, os, uptime
    pub cpu: CpuStats,                  // clusters: Vec<CpuCluster{kind: E|P|All, usage, mhz: Option}>, per_core
    pub gpu: Option<GpuStats>,          // usage, mhz, watts, mem_used
    pub ane: Option<AneStats>,          // watts
    pub media: Option<f64>,             // GB/s, best-effort
    pub power: Option<PowerStats>,      // cpu_w, gpu_w, ane_w, dram_w, system_w
    pub mem: MemStats,                  // total, used, wired/active/compressed/free (Option), pressure, swap, app, cached
    pub bandwidth: Option<Bandwidth>,   // total, cpu, gpu, media, ane, other (GB/s)
    pub net: NetStats,                  // rx/tx rate total + per interface
    pub disk: DiskStats,                // read/write rate, used/total, since-boot totals
    pub sensors: Vec<SensorGroup>,      // name, kind, readings[(label, °C)]; fans
    pub procs: Vec<ProcInfo>,           // pid, ppid, name, cmd, user, cpu%, mem, threads, start
    pub ai: AiStats,                    // detected runtimes, top AI process, model budget
}
```

## 4. Data sources

### macOS (Apple Silicon)

- **IOReport** (private `libIOReport.dylib`, called via `extern "C"`):
  subscribe once to groups `Energy Model`, `CPU Stats` / `CPU Core Performance
  States`, `GPU Stats` / `GPU Performance States`. Each tick: create sample,
  diff with the previous one.
  - usage % = 1 − residency in IDLE/OFF states
  - MHz = residency-weighted average over the DVFS frequency table read from
    IORegistry `pmgr` (`voltage-states*` properties)
  - watts = Δenergy / Δt (unit taken from the channel: mJ/µJ/nJ)
- **Memory bandwidth (best-effort):** `AMC Stats` / `PMP` IOReport groups when
  present. **Known risk:** undocumented and varies across chips/OS versions; on
  failure the bandwidth block shows `—`.
- **GPU memory:** `IOAccelerator` `PerformanceStatistics` → "In use system memory".
- **SMC** (`AppleSMC` via IOKit): enumerate keys at startup; temperatures of type
  `flt ` grouped by prefix (`Tp*`/`Te*` → CPU, `Tg*` → GPU, `TB*` → Battery,
  others → Other); fans via `FNum`, `F<n>Ac`. Fallback: `IOHIDEventSystemClient`
  temperature sensors.
- **VM:** `host_statistics64` (wired/active/compressed/free/purgeable),
  `kern.memorystatus_level` → pressure, `vm.swapusage`.
- **SoC info:** `sysctl` `machdep.cpu.brand_string`, `hw.perflevel{0,1}.logicalcpu`.

### Linux

- **CPU:** `/proc/stat` per core; `scaling_cur_freq`; hybrid detection via
  `/sys/devices/cpu_core|cpu_atom/cpus` or `cpu_capacity`; otherwise one `All`
  cluster.
- **Sensors:** `/sys/class/hwmon/*` grouped by `name`
  (`coretemp`/`k10temp`/`zenpower` → CPU, `amdgpu`/`nvidia` → GPU, `nvme` →
  Disk, `BAT*` → Battery, else Other); fans from `fan*_input`.
- **Power:** RAPL `/sys/class/powercap/intel-rapl:*/energy_uj` when readable;
  otherwise `None` with a hint in help.
- **GPU:** NVML via `nvml-wrapper` (usage, clocks, power, memory); AMD via
  `/sys/class/drm/card*/device/{gpu_busy_percent,mem_info_vram_used,pp_dpm_sclk}`
  and its hwmon. First GPU only in v1.

### Common (`sysinfo`)

Processes, networks (rates from deltas), disks (usage + I/O rates from process
disk usage aggregation / `/proc/diskstats` on Linux, IOKit block storage stats
on macOS), uptime, load.

### AI

- Runtime detection by process name match: `ollama`, `llama-server`,
  `llama-cli`, `mlx_lm`, `LM Studio`, `lms`, `vllm`.
- Workload state per engine (CPU/GPU/ANE/Media/Mem): `Active` when above a
  threshold (CPU > 10 %, GPU > 10 %, ANE > 0.1 W, Media > 0.1 GB/s, pressure
  > 50 % → `Elevated`), else `idle` / `Normal`.
- Model budget: usable memory (macOS: 75 % of total RAM − used; Linux: free
  VRAM, or available RAM without GPU) ÷ 0.6 bytes/param (Q4_K_M), rounded down
  to the nearest of 1, 2, 3, 7, 8, 13, 14, 32, 70, 120 B.

## 5. UI

### Layout

Header line: name, chip, core counts, system W, uptime. Then a grid of panels:

| Row | Left | Right |
|---|---|---|
| 1 | [1] AI workload | [2] AI runtime |
| 2 | [3] CPU (bars + braille history E/P) | [4] GPU / Media / ANE |
| 3 | [5] Memory & bandwidth | [6] Network & disk |
| 4 | [7] Sensors (collapsible groups) | [8] Processes |

- Width ≥ 120 cols: two columns. Narrower: single column, compact panels.
- Panels without data collapse to one line or hide (`panels.hide_empty`).
- Focused panel has highlighted border; `z`/`Enter` maximizes it with extra
  detail (CPU per core, every sensor, all process columns); `Esc` restores.
- Status bar at bottom: context key hints + transient messages (3 s).

### Keys

| Context | Key | Action |
|---|---|---|
| Global | `Tab` / `Shift-Tab`, `1`–`8` | focus panel |
| | `z` / `Enter`, `Esc` | zoom / unzoom |
| | `p` | pause sampling |
| | `+` / `-` | interval (250 ms – 10 s) |
| | `?` | help overlay (includes missing-metric reasons) |
| | `q`, `Ctrl-C` | quit |
| Sensors | `↑↓`, `Space` | move / expand-collapse group |
| Processes | `↑↓`, `j`/`k`, `PgUp`/`PgDn`, `g`/`G` | navigate |
| | `/` | incremental name filter (`Esc` clears) |
| | `s` / `S` | cycle sort column (CPU, MEM, PID, NAME) / reverse |
| | `t` | tree view toggle |
| | `k` / `K` | SIGTERM / SIGKILL with confirmation modal |
| | `Enter` (zoomed) | details: cmdline, user, threads, memory, start time |

### Themes

`dark` (default, mimics the screenshot palette), `light`, `mono`. Individual
colors overridable in `[theme.colors]`.

### Config file

```toml
interval_ms = 1000
theme = "dark"

[panels]
hide_empty = true
hidden = []            # e.g. ["ai_runtime"]

[processes]
sort = "cpu"
reverse = false
tree = false

[theme.colors]
# accent = "#5b8def"
```

CLI flags override config. Invalid config → error message and exit code 2.

## 6. Error handling

- Each source returns `Result`; errors become `None` in the snapshot and are
  logged once per source to `--log <file>` (never drawn into the TUI).
- If no platform source works, continue with `sysinfo` only and show a status
  bar notice.
- Kill errors (`EPERM`, `ESRCH`) become status bar messages.
- Terminal is always restored: drop guard plus panic hook that leaves raw mode
  and the alternate screen before printing the panic.
- Unsupported OS at compile time → only common + demo collectors compiled.

## 7. Testing

- **Unit:** pure parsers and math with fixtures in `tests/fixtures/`:
  `/proc/stat`, hwmon trees, amdgpu sysfs, IOReport residency → usage/MHz,
  energy → watts, model budget, process sort/filter/tree, config parsing,
  history ring buffer.
- **UI:** `ratatui::backend::TestBackend` rendering of demo snapshots at 80×24
  and 160×48; focus, zoom, filter and kill-modal transitions driven by
  synthetic key events.
- **Smoke (platform-gated, `#[ignore]` in CI where hardware is missing):**
  real collector produces two snapshots without panicking.
- **CI:** GitHub Actions on `macos-14` and `ubuntu-latest`: `cargo fmt --check`,
  `cargo clippy -- -D warnings`, `cargo test`. Release with `cargo-dist` for
  darwin/linux × arm64/amd64.

## 8. Risks

| Risk | Mitigation |
|---|---|
| IOReport / SMC are private and may change | Every channel optional; `—` on failure; smoke test on macOS CI |
| Memory bandwidth counters absent on some chips | Best-effort, explicitly allowed to be `—` |
| RAPL root-only on modern kernels | Show `—` + explanation in help |
| NVML absent | Loaded at runtime; fall back to AMD sysfs or hide GPU panel |
