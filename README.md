# lazytop

A lazy-style terminal system monitor for **macOS (Apple Silicon)** and **Linux**, written in Rust with [ratatui](https://ratatui.rs).

lazytop shows CPU efficiency/performance clusters, GPU / Media / Neural Engine, memory and memory bandwidth, network and disk, temperatures and fans, power, processes and a local-AI overview — without root.

## Features

- **CPU**: E/P cluster usage and frequency (Apple Silicon via IOReport; Intel hybrid via sysfs), per-core view when zoomed, braille history.
- **GPU / Media / ANE**: Apple GPU usage, MHz, watts and memory; ANE power; media bandwidth. Linux: NVIDIA (NVML) or AMD (amdgpu sysfs).
- **Memory**: wired / active / compressed / free, pressure, app memory, cached, swap; memory bandwidth per agent (CPU, GPU, Media, ANE, other) on Apple Silicon.
- **Network & disk**: per-interface rates, disk read/write rates, usage and totals since boot.
- **Sensors**: temperatures grouped by CPU / GPU / Battery / Disk (SMC on macOS, hwmon on Linux), fans.
- **Processes**: sort (CPU, MEM, PID, NAME), incremental filter, tree view, details, SIGTERM/SIGKILL with confirmation.
- **AI**: detects local runtimes (ollama, llama.cpp, MLX, LM Studio, vLLM) and estimates the largest Q4_K_M model that fits in free memory.
- **Power**: system watts (SMC `PSTR`) and GPU / ANE watts on Apple Silicon; RAPL on Linux when readable.
- Missing metrics show `—`; press `?` to see why.

## Installation

### With mise

```bash
mise use -g github:celson/lazytop
```

### From source

```bash
git clone https://github.com/celson/lazytop.git
cd lazytop
mise run build        # target/release/lazytop
```

Other mise tasks: `run`, `demo`, `test`, `lint`, `install`, `clean`.

## Usage

```bash
lazytop                          # live
lazytop --demo                   # synthetic data
lazytop -i 500                   # refresh every 500 ms
lazytop --theme light
lazytop --theme catppuccin       # mocha; also catppuccin-macchiato, -frappe, -latte
lazytop --log /tmp/lazytop.log   # diagnostics about unavailable sources
```

## Keys

| Key | Action |
|---|---|
| `Tab` / `Shift-Tab`, `1`–`8` | focus panel |
| `z` / `Enter`, `Esc` | zoom / unzoom |
| `p` | pause |
| `+` / `-` | faster / slower refresh |
| `?` | help |
| `q`, `Ctrl-C` | quit |
| Sensors: `↑↓`, `Space` | select / expand group |
| Processes: `↑↓` `j`, `PgUp/PgDn`, `g/G` | navigate |
| Processes: `/` | filter |
| Processes: `s` / `S` | sort column / reverse |
| Processes: `t` | tree |
| Processes: `k` / `K` | SIGTERM / SIGKILL (confirm) |
| Processes (zoomed): `Enter` | details |

## Configuration

`~/.config/lazytop/config.toml`:

```toml
interval_ms = 1000
theme = "dark"          # dark | light | mono | catppuccin (= catppuccin-mocha)
                        # | catppuccin-macchiato | catppuccin-frappe | catppuccin-latte

[panels]
hide_empty = true
hidden = []             # ai_workload, ai_runtime, cpu, gpu, memory, netdisk, sensors, processes

[processes]
sort = "cpu"            # cpu | mem | pid | name
reverse = false
tree = false

[colors]
# accent = "#5b8def"
```

An invalid config file makes lazytop exit with code 2 and a message.

## How metrics are read

- **macOS**: IOReport (the private framework `powermetrics` uses) for CPU/GPU residency and frequency, GPU/ANE energy and the `PMP / DCS BW` memory-bandwidth histograms; IOKit SMC for temperatures, fans and system power (`PSTR`); `host_statistics64` and `sysctl` for memory; IOKit block storage statistics for disk. No `sudo` needed.
  - IOReport and SMC are undocumented and change between releases. On recent macOS the CPU/DRAM energy counters stop updating, so CPU watts show `—`.
  - Bandwidth values are estimates from histogram bins; the total is the sum of agents.
- **Linux**: `/proc`, `/sys/class/hwmon`, `/sys/class/powercap` (RAPL), NVML, `/sys/class/drm` (amdgpu).
- **Both**: [sysinfo](https://crates.io/crates/sysinfo) for processes, network and disks.

## License

MIT
