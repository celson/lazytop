use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostInfo {
    pub chip: String,
    pub os: String,
    pub hostname: String,
    pub cores_e: Option<usize>,
    pub cores_p: Option<usize>,
    pub cores_total: usize,
    pub uptime: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClusterKind {
    Efficiency,
    Performance,
    All,
}

impl ClusterKind {
    pub fn label(self) -> &'static str {
        match self {
            ClusterKind::Efficiency => "Efficiency",
            ClusterKind::Performance => "Performance",
            ClusterKind::All => "CPU",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CpuCluster {
    pub kind: ClusterKind,
    /// 0.0..=1.0
    pub usage: f64,
    pub mhz: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuStats {
    pub clusters: Vec<CpuCluster>,
    /// 0.0..=1.0 per logical core
    pub per_core: Vec<f64>,
    /// 0.0..=1.0 average
    pub total: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpuStats {
    pub name: String,
    /// 0.0..=1.0
    pub usage: f64,
    pub mhz: Option<u32>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PowerStats {
    pub cpu_w: Option<f64>,
    pub gpu_w: Option<f64>,
    pub ane_w: Option<f64>,
    pub dram_w: Option<f64>,
    pub system_w: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemStats {
    pub total: u64,
    pub used: u64,
    pub free: u64,
    pub wired: Option<u64>,
    pub active: Option<u64>,
    pub compressed: Option<u64>,
    pub app: Option<u64>,
    pub cached: Option<u64>,
    pub swap_used: u64,
    pub swap_total: u64,
    /// 0.0..=1.0
    pub pressure: Option<f64>,
}

/// Bytes per second.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bandwidth {
    pub total: f64,
    pub cpu: f64,
    pub gpu: f64,
    pub media: f64,
    pub ane: f64,
    pub other: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetIface {
    pub name: String,
    pub rx: f64,
    pub tx: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetStats {
    pub rx: f64,
    pub tx: f64,
    pub ifaces: Vec<NetIface>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiskStats {
    pub read: Option<f64>,
    pub write: Option<f64>,
    pub used: u64,
    pub total: u64,
    pub read_total: Option<u64>,
    pub write_total: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SensorKind {
    Cpu,
    Gpu,
    Battery,
    Disk,
    Other,
}

impl SensorKind {
    pub fn label(self) -> &'static str {
        match self {
            SensorKind::Cpu => "CPU",
            SensorKind::Gpu => "GPU",
            SensorKind::Battery => "Battery",
            SensorKind::Disk => "Disk",
            SensorKind::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SensorGroup {
    pub kind: SensorKind,
    /// (label, °C)
    pub readings: Vec<(String, f64)>,
}

impl SensorGroup {
    pub fn avg(&self) -> Option<f64> {
        if self.readings.is_empty() {
            return None;
        }
        Some(self.readings.iter().map(|r| r.1).sum::<f64>() / self.readings.len() as f64)
    }
    pub fn max(&self) -> Option<f64> {
        self.readings.iter().map(|r| r.1).reduce(f64::max)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fan {
    pub label: String,
    pub rpm: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcInfo {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub cmd: String,
    pub user: String,
    /// percent of one core (can exceed 100)
    pub cpu: f32,
    pub mem: u64,
    pub threads: Option<usize>,
    /// seconds since epoch
    pub start_time: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineState {
    #[default]
    Idle,
    Active,
    Normal,
    Elevated,
}

impl EngineState {
    pub fn label(self) -> &'static str {
        match self {
            EngineState::Idle => "idle",
            EngineState::Active => "Active",
            EngineState::Normal => "Normal",
            EngineState::Elevated => "Elevated",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Engines {
    pub cpu: EngineState,
    pub gpu: EngineState,
    pub ane: EngineState,
    pub media: EngineState,
    pub mem: EngineState,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiRuntime {
    pub name: String,
    pub pid: u32,
    pub cpu: f32,
    pub mem: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiStats {
    pub runtimes: Vec<AiRuntime>,
    pub top_proc: Option<(String, f32)>,
    pub engines: Engines,
    /// true when any AI runtime is consuming CPU or GPU is active with a runtime present
    pub busy: bool,
    /// largest model size (billions of params, Q4_K_M) that fits
    pub budget_b: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub host: HostInfo,
    pub cpu: CpuStats,
    pub gpu: Option<GpuStats>,
    pub power: Option<PowerStats>,
    pub mem: MemStats,
    pub bandwidth: Option<Bandwidth>,
    pub net: NetStats,
    pub disk: DiskStats,
    pub sensors: Vec<SensorGroup>,
    pub fans: Vec<Fan>,
    pub procs: Vec<ProcInfo>,
    pub ai: AiStats,
}
