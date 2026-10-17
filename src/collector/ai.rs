use crate::model::*;

/// Lower-cased substrings matched against process names.
pub const RUNTIME_PATTERNS: [&str; 7] = [
    "ollama",
    "llama-server",
    "llama-cli",
    "mlx_lm",
    "lm studio",
    "lms",
    "vllm",
];

/// Bytes per parameter for Q4_K_M including overhead.
const BYTES_PER_PARAM: f64 = 0.6;
const SIZES_B: [u32; 10] = [1, 2, 3, 7, 8, 13, 14, 32, 70, 120];

pub fn model_budget_b(usable_bytes: u64) -> Option<u32> {
    let params_b = usable_bytes as f64 / BYTES_PER_PARAM / 1e9;
    SIZES_B
        .iter()
        .rev()
        .find(|s| **s as f64 <= params_b)
        .copied()
}

fn level(active: bool) -> EngineState {
    if active {
        EngineState::Active
    } else {
        EngineState::Idle
    }
}

pub fn analyze(s: &Snapshot) -> AiStats {
    let runtimes: Vec<AiRuntime> = s
        .procs
        .iter()
        .filter(|p| {
            let n = p.name.to_lowercase();
            RUNTIME_PATTERNS.iter().any(|pat| n.contains(pat))
        })
        .map(|p| AiRuntime {
            name: p.name.clone(),
            pid: p.pid,
            cpu: p.cpu,
            mem: p.mem,
        })
        .collect();

    let gpu_usage = s.gpu.as_ref().map(|g| g.usage).unwrap_or(0.0);
    let ane_w = s.power.as_ref().and_then(|p| p.ane_w).unwrap_or(0.0);
    let media = s.bandwidth.as_ref().map(|b| b.media).unwrap_or(0.0);
    let engines = Engines {
        cpu: level(s.cpu.total > 0.10),
        gpu: level(gpu_usage > 0.10),
        ane: level(ane_w > 0.1),
        media: level(media > 0.1e9),
        mem: if s.mem.pressure.unwrap_or(0.0) > 0.5 {
            EngineState::Elevated
        } else {
            EngineState::Normal
        },
    };

    let top_proc = s
        .procs
        .iter()
        .max_by(|a, b| a.cpu.total_cmp(&b.cpu))
        .map(|p| (p.name.clone(), p.cpu));

    let busy = runtimes.iter().any(|r| r.cpu > 10.0) || (!runtimes.is_empty() && gpu_usage > 0.10);

    let usable = match &s.gpu {
        Some(GpuStats {
            mem_total: Some(t),
            mem_used,
            ..
        }) => t.saturating_sub(mem_used.unwrap_or(0)),
        _ => ((s.mem.total as f64 * 0.75) as u64).saturating_sub(s.mem.used),
    };

    AiStats {
        runtimes,
        top_proc,
        engines,
        busy,
        budget_b: model_budget_b(usable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    fn proc_(name: &str, cpu: f32) -> ProcInfo {
        ProcInfo {
            pid: 1,
            name: name.into(),
            cpu,
            mem: GB,
            ..Default::default()
        }
    }

    #[test]
    fn budget_rounds_down_to_known_sizes() {
        assert_eq!(model_budget_b(0), None);
        assert_eq!(model_budget_b(GB / 2), None); // < 1B
        assert_eq!(model_budget_b((1.3 * GB as f64) as u64), Some(2));
        assert_eq!(model_budget_b(5 * GB), Some(8));
        assert_eq!(model_budget_b(500 * GB), Some(120));
    }

    #[test]
    fn detects_runtimes_case_insensitive() {
        let s = Snapshot {
            procs: vec![
                proc_("Ollama", 50.0),
                proc_("bash", 1.0),
                proc_("LM Studio Helper", 2.0),
            ],
            ..Default::default()
        };
        let a = analyze(&s);
        let names: Vec<_> = a.runtimes.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Ollama", "LM Studio Helper"]);
        assert!(a.busy);
    }

    #[test]
    fn engine_states_and_top_proc() {
        let s = Snapshot {
            cpu: CpuStats {
                total: 0.13,
                ..Default::default()
            },
            gpu: Some(GpuStats {
                usage: 0.05,
                ..Default::default()
            }),
            power: Some(PowerStats {
                ane_w: Some(0.0),
                ..Default::default()
            }),
            mem: MemStats {
                total: 18 * GB,
                used: 10 * GB,
                pressure: Some(0.6),
                ..Default::default()
            },
            procs: vec![proc_("a", 3.0), proc_("screencaptureui", 13.0)],
            ..Default::default()
        };
        let a = analyze(&s);
        assert_eq!(a.engines.cpu, EngineState::Active);
        assert_eq!(a.engines.gpu, EngineState::Idle);
        assert_eq!(a.engines.ane, EngineState::Idle);
        assert_eq!(a.engines.mem, EngineState::Elevated);
        assert_eq!(a.top_proc, Some(("screencaptureui".into(), 13.0)));
        assert!(!a.busy);
    }

    #[test]
    fn budget_uses_75_percent_on_unified_memory() {
        // no discrete GPU memory → 0.75*total - used
        let s = Snapshot {
            mem: MemStats {
                total: 18 * GB,
                used: 11 * GB,
                ..Default::default()
            },
            ..Default::default()
        };
        // 13.5 - 11 = 2.5 GB → 4.1B → 3
        assert_eq!(analyze(&s).budget_b, Some(3));
    }

    #[test]
    fn budget_uses_free_vram_on_discrete_gpu() {
        let s = Snapshot {
            gpu: Some(GpuStats {
                mem_used: Some(2 * GB),
                mem_total: Some(12 * GB),
                ..Default::default()
            }),
            mem: MemStats {
                total: 64 * GB,
                used: 8 * GB,
                ..Default::default()
            },
            ..Default::default()
        };
        // 10 GB free → 16.6B → 14
        assert_eq!(analyze(&s).budget_b, Some(14));
    }
}
