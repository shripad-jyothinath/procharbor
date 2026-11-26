use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::desktop::home_dir;
use crate::fmt::{freq_to_channel, unescape_octal};

pub const HISTORY_LEN: usize = 120;

pub fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn clk_tck() -> f64 {
    let v = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if v > 0 { v as f64 } else { 100.0 }
}

pub fn page_size() -> u64 {
    let v = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if v > 0 { v as u64 } else { 4096 }
}

pub fn my_uid() -> u32 {
    unsafe { libc::getuid() }
}

pub fn state_label(st: &str) -> &'static str {
    match st {
        "R" => "Running",
        "S" => "Sleeping",
        "D" => "Disk sleep",
        "I" => "Idle",
        "T" | "t" => "Suspended",
        "Z" => "Zombie",
        "X" | "x" => "Dead",
        "W" => "Paging",
        "P" => "Parked",
        _ => "Unknown",
    }
}

pub fn engine_name(eng: &str) -> &str {
    match eng {
        "render" | "rcs" => "3D",
        "video" | "video-enhance" | "vcs" => "Video",
        "copy" | "bcs" => "Copy",
        "vecs" | "ccs" => "Compute",
        other => other,
    }
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct CpuInfo {
    pub model: String,
    pub util: f64,
    pub per_core: Vec<f64>,
    pub freq: f64,
    pub freqs: Vec<f64>,
    pub base_mhz: f64,
    pub base_e_mhz: f64,
    pub max_policy_mhz: f64,
    pub cores: usize,
    pub logical: usize,
    pub sockets: usize,
    pub no_turbo: Option<i64>,
    pub virtualization: bool,
    pub temp: Option<f64>,
    pub load: [f64; 3],
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct MemoryInfo {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub free: u64,
    pub cached: u64,
    pub buffers: u64,
    pub committed: u64,
    pub commit_limit: u64,
    pub slab: u64,
    pub kernel: u64,
    pub dirty: u64,
    pub writeback: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    pub swap_used: u64,
    pub percent: f64,
    // Hardware details (Windows Task Manager parity)
    pub speed_mts: Option<u64>,
    pub mem_type: Option<String>,
    pub manufacturer: Option<String>,
    pub form_factor: Option<String>,
    pub slots_used: usize,
    pub slots_total: usize,
    pub hw_reserved: u64,
    pub installed: u64,
    pub max_capacity: Option<u64>,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct DiskInfo {
    pub name: String,
    pub model: String,
    pub read: f64,
    pub write: f64,
    pub active: f64,
    pub iops: f64,
    pub response: f64,
    pub size: u64,
    pub temp: Option<f64>,
}

#[derive(Clone, Default)]
pub struct VolumeInfo {
    pub device: String,
    pub mount: String,
    pub fstype: String,
    pub total: u64,
    pub free: u64,
    pub used: u64,
    pub percent: f64,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct NetIfaceInfo {
    pub name: String,
    pub rx: f64,
    pub tx: f64,
    pub rx_total: u64,
    pub tx_total: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub mac: String,
    pub mtu: Option<i64>,
    pub state: String,
    pub ipv4: Option<String>,
    pub ipv6: Vec<String>,
    pub wireless: bool,
    pub driver: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct WifiInfo {
    pub iface: String,
    pub ssid: Option<String>,
    pub signal_dbm: Option<f64>,
    pub signal_pct: Option<f64>,
    pub link_mbps: Option<f64>,
    pub freq_ghz: Option<f64>,
    pub channel: Option<u32>,
    pub ap: Option<String>,
    pub txpower: Option<i64>,
    pub powersave: Option<String>,
    pub driver: Option<String>,
    pub mac: String,
    pub ipv4: Option<String>,
    pub ipv6: Vec<String>,
    pub temp: Option<f64>,
}

#[derive(Clone, Default)]
pub struct NetInfo {
    pub interfaces: Vec<NetIfaceInfo>,
    pub wifi: Option<WifiInfo>,
}

#[derive(Clone, Default)]
pub struct GpuInfo {
    pub name: String,
    pub util: f64,
    pub freq: Option<i64>,
    pub freq_act: Option<i64>,
    pub freq_max: Option<i64>,
    pub driver: String,
}

#[derive(Clone, Default)]
pub struct Totals {
    pub processes: usize,
    pub threads: u64,
    pub handles: u64,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct SystemInfo {
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disks: Vec<DiskInfo>,
    pub volumes: Vec<VolumeInfo>,
    pub net: NetInfo,
    pub gpu: Option<GpuInfo>,
    pub uptime: f64,
    pub totals: Totals,
}

#[derive(Clone, Default)]
pub struct HistoryBuffers {
    pub cpu: Vec<f64>,
    pub cores: Vec<Vec<f64>>,
    pub mem: Vec<f64>,
    pub disk_r: Vec<f64>,
    pub disk_w: Vec<f64>,
    pub net_rx: Vec<f64>,
    pub net_tx: Vec<f64>,
    pub gpu: Vec<f64>,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub app: String,
    pub exe: String,
    pub user: String,
    pub uid: u32,
    pub state: String,
    pub status: String,
    pub cpu: f64,
    pub mem: u64,
    pub rss: u64,
    pub vmem: u64,
    pub threads: u64,
    pub handles: u64,
    pub nice: i64,
    pub priority: i64,
    pub started: f64,
    pub disk_r: u64,
    pub disk_w: u64,
    pub net_up: u64,
    pub net_down: u64,
    pub gpu: f64,
    pub gpu_engine: Option<String>,
    pub cmdline: String,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct AppHistoryRow {
    pub name: String,
    pub cpu: f64,
    pub mem: u64,
    pub gpu: f64,
    pub procs: usize,
    pub disk_r: u64,
    pub disk_w: u64,
    pub up: u64,
    pub down: u64,
    pub cpu_s: f64,
    pub total_disk_r: u64,
    pub total_disk_w: u64,
    pub total_up: u64,
    pub total_down: u64,
}

#[derive(Clone, Default)]
pub struct UserRow {
    pub uid: u32,
    pub user: String,
    pub session: String,
    pub tty: String,
    pub state: String,
    pub cpu: f64,
    pub mem: u64,
    pub disk: u64,
    pub up: u64,
    pub down: u64,
    pub procs: usize,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct ServiceRow {
    pub unit: String,
    pub description: String,
    pub active: String,
    pub sub: String,
    pub load: String,
    pub startup: String,
    pub scope: String,
    pub pid: Option<u32>,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct StartupRow {
    pub name: String,
    pub status: String,
    pub entry_type: String,
    pub scope: String,
    pub command: String,
    pub description: String,
    pub source: String,
    pub unit_or_file: String,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct Snapshot {
    pub ts: f64,
    pub started: f64,
    pub sys_ts: f64,
    pub proc_ts: f64,
    pub proc_samples: u64,
    pub system: Option<SystemInfo>,
    pub history: HistoryBuffers,
    pub processes: Vec<ProcessInfo>,
    pub apps: Vec<AppHistoryRow>,
    pub users: Vec<UserRow>,
    pub totals: Totals,
}

#[derive(Clone)]
struct ProcStat {
    name: String,
    state: String,
    ppid: u32,
    utime: u64,
    stime: u64,
    priority: i64,
    nice: i64,
    threads: u64,
    starttime: u64,
    vsize: u64,
    rss: u64,
    pss: u64,
}

#[derive(Clone)]
struct ProcMeta {
    starttime: u64,
    uid: u32,
    user: String,
    exe: String,
    cmdline: String,
    app: Option<String>,
    ts: f64,
}

#[derive(Clone, Copy)]
struct PrevProc {
    starttime: u64,
    ticks: u64,
    rd: u64,
    wr: u64,
}

#[derive(Clone, Default)]
struct AppTotal {
    cpu_s: f64,
    disk_r: u64,
    disk_w: u64,
    up: u64,
    down: u64,
}

#[derive(Clone)]
struct SessionInfo {
    uid: u32,
    user: String,
    session: String,
    tty: String,
    state: String,
}

#[derive(Default)]
struct HwmonPaths {
    cpu: Option<String>,
    nvme: Option<String>,
    wifi: Option<String>,
}

#[derive(Deserialize)]
struct SystemctlUnit {
    #[serde(default)]
    unit: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    active: String,
    #[serde(default)]
    sub: String,
    #[serde(default)]
    load: String,
}

#[derive(Deserialize)]
struct SystemctlUnitFile {
    #[serde(default)]
    unit_file: String,
    #[serde(default)]
    state: String,
}

struct SharedState {
    system: Option<SystemInfo>,
    sys_ts: f64,
    proc_list: Vec<ProcessInfo>,
    proc_ts: f64,
    proc_samples: u64,
    totals: Totals,
    hist: HistoryBuffers,
    app_totals: HashMap<String, AppTotal>,
    pid_app: HashMap<u32, String>,
    net_proc: HashMap<u32, (u64, u64)>,
    gpu_proc: HashMap<u32, (f64, Option<String>)>,
    gpu_total: f64,
    fdcount: HashMap<u32, u64>,
    sessions_ts: f64,
    sessions_data: Vec<SessionInfo>,
    services_ts: f64,
    services_data: Vec<ServiceRow>,
    startup_ts: f64,
    startup_data: Vec<StartupRow>,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            system: None,
            sys_ts: 0.0,
            proc_list: Vec::new(),
            proc_ts: 0.0,
            proc_samples: 0,
            totals: Totals::default(),
            hist: HistoryBuffers::default(),
            app_totals: HashMap::new(),
            pid_app: HashMap::new(),
            net_proc: HashMap::new(),
            gpu_proc: HashMap::new(),
            gpu_total: 0.0,
            fdcount: HashMap::new(),
            sessions_ts: 0.0,
            sessions_data: Vec::new(),
            services_ts: 0.0,
            services_data: Vec::new(),
            startup_ts: 0.0,
            startup_data: Vec::new(),
        }
    }
}

struct SamplerInternals {
    sys_prev_cpu: Option<HashMap<String, (u64, u64)>>,
    disk_prev: HashMap<String, (u64, u64, u64, u64, u64)>,
    disk_ts: Option<f64>,
    net_if_prev: HashMap<String, (u64, u64)>,
    disk_models: HashMap<String, String>,
    hwmon: HwmonPaths,
    uid_names: HashMap<u32, String>,
    meta: HashMap<u32, ProcMeta>,
    prev_proc: HashMap<u32, PrevProc>,
    gpu_prev: HashMap<u32, HashMap<String, u64>>,
    gpu_ts: f64,
    prev_net: HashMap<(u32, u32, String, String, String), (u32, u64, u64)>,
    net_ts: f64,
    scan_count: u64,
}

impl Default for SamplerInternals {
    fn default() -> Self {
        Self {
            sys_prev_cpu: None,
            disk_prev: HashMap::new(),
            disk_ts: None,
            net_if_prev: HashMap::new(),
            disk_models: HashMap::new(),
            hwmon: HwmonPaths::default(),
            uid_names: HashMap::new(),
            meta: HashMap::new(),
            prev_proc: HashMap::new(),
            gpu_prev: HashMap::new(),
            gpu_ts: 0.0,
            prev_net: HashMap::new(),
            net_ts: 0.0,
            scan_count: 0,
        }
    }
}

pub struct Sampler {
    start_ts: f64,
    pub ncpu: usize,
    cpu_model: String,
    phys_cores: usize,
    sockets: usize,
    cpu_flags: HashSet<String>,
    boot_time: f64,
    gpu_name: Option<String>,
    gpu_card: Option<String>,
    mem_hw: MemoryHardware,
    clk_tck: f64,
    page_size: u64,
    stop: Arc<AtomicBool>,
    sample_seq: AtomicU64,
    shared: Arc<Mutex<SharedState>>,
    sys_internals: Arc<Mutex<SamplerInternals>>,
    proc_internals: Arc<Mutex<SamplerInternals>>,
}

impl Sampler {
    pub fn new() -> Arc<Self> {
        let start_ts = now_secs();
        let ncpu = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .max(1);
        let cpu_model = read_cpu_model();
        let phys_cores = read_physical_cores(ncpu);
        let sockets = read_sockets();
        let cpu_flags = read_cpu_flags();
        let boot_time = read_boot_time();
        let (gpu_name, gpu_card) = find_gpu();
        let mem_hw = read_memory_hardware();

        let mut sys_int = SamplerInternals::default();
        refresh_hwmon(&mut sys_int.hwmon);

        Arc::new(Self {
            start_ts,
            ncpu,
            cpu_model,
            phys_cores,
            sockets,
            cpu_flags,
            boot_time,
            gpu_name,
            gpu_card,
            mem_hw,
            clk_tck: clk_tck(),
            page_size: page_size(),
            stop: Arc::new(AtomicBool::new(false)),
            sample_seq: AtomicU64::new(0),
            shared: Arc::new(Mutex::new(SharedState::default())),
            sys_internals: Arc::new(Mutex::new(sys_int)),
            proc_internals: Arc::new(Mutex::new(SamplerInternals::default())),
        })
    }

    pub fn start(self: &Arc<Self>, wakeup: Option<Arc<dyn Fn() + Send + Sync>>) {
        // Initial quick samples so CPU% and system metrics populate fast
        self.sample_system();
        self.scan_gpu(now_secs());
        self.sample_processes();

        let s1 = Arc::clone(self);
        let w1 = wakeup.clone();
        thread::Builder::new()
            .name("sys-sampler".to_string())
            .spawn(move || {
                while !s1.stop.load(Ordering::Relaxed) {
                    let t0 = now_secs();
                    s1.sample_system();
                    if let Some(ref cb) = w1 {
                        cb();
                    }
                    let elapsed = now_secs() - t0;
                    let sleep_s = (1.0 - elapsed).max(0.05);
                    thread::sleep(Duration::from_secs_f64(sleep_s));
                }
            })
            .ok();

        let s2 = Arc::clone(self);
        let w2 = wakeup;
        thread::Builder::new()
            .name("proc-sampler".to_string())
            .spawn(move || {
                // Short delay then second sample so CPU deltas appear right away
                thread::sleep(Duration::from_millis(250));
                while !s2.stop.load(Ordering::Relaxed) {
                    let t0 = now_secs();
                    s2.sample_processes();
                    if let Some(ref cb) = w2 {
                        cb();
                    }
                    let elapsed = now_secs() - t0;
                    let sleep_s = (1.0 - elapsed).max(0.05);
                    thread::sleep(Duration::from_secs_f64(sleep_s));
                }
            })
            .ok();
    }

    pub fn seq(&self) -> u64 {
        self.sample_seq.load(Ordering::Acquire)
    }

    pub fn stop_all(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn sample_system(&self) {
        let now = now_secs();
        let mut int = self.sys_internals.lock().unwrap();

        let cpu_now = read_cpu_times();
        let mut per_core = vec![0.0; self.ncpu];
        let mut util = 0.0;
        if let Some(ref prev) = int.sys_prev_cpu {
            for (i, core_val) in per_core.iter_mut().enumerate() {
                let k = format!("cpu{}", i);
                let (t0, i0) = prev.get(&k).copied().unwrap_or((0, 0));
                let (t1, i1) = cpu_now.get(&k).copied().unwrap_or((0, 0));
                let dt = t1.saturating_sub(t0);
                if dt > 0 {
                    let idle_d = i1.saturating_sub(i0) as f64;
                    *core_val = (100.0 * (1.0 - idle_d / dt as f64)).clamp(0.0, 100.0);
                }
            }
            let (t0, i0) = prev.get("cpu").copied().unwrap_or((0, 0));
            let (t1, i1) = cpu_now.get("cpu").copied().unwrap_or((0, 0));
            let dt = t1.saturating_sub(t0);
            if dt > 0 {
                let idle_d = i1.saturating_sub(i0) as f64;
                util = (100.0 * (1.0 - idle_d / dt as f64)).clamp(0.0, 100.0);
            }
        }
        int.sys_prev_cpu = Some(cpu_now);

        let mut freqs = Vec::with_capacity(self.ncpu);
        let mut bases = Vec::new();
        let mut maxes = Vec::new();
        for i in 0..self.ncpu {
            let v = read_int(&format!(
                "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq",
                i
            ))
            .unwrap_or(0) as f64;
            freqs.push(v / 1000.0);
            if let Some(b) = read_int(&format!(
                "/sys/devices/system/cpu/cpu{}/cpufreq/base_frequency",
                i
            )) {
                if b > 0 {
                    bases.push(b);
                }
            }
            if let Some(m) = read_int(&format!(
                "/sys/devices/system/cpu/cpu{}/cpufreq/cpuinfo_max_freq",
                i
            )) {
                if m > 0 {
                    maxes.push(m);
                }
            }
        }
        let freq_avg = if freqs.is_empty() {
            0.0
        } else {
            freqs.iter().sum::<f64>() / freqs.len() as f64
        };
        let base_p = bases
            .iter()
            .copied()
            .max()
            .map(|v| v as f64 / 1000.0)
            .unwrap_or(0.0);
        let base_e = bases
            .iter()
            .copied()
            .min()
            .map(|v| v as f64 / 1000.0)
            .unwrap_or(0.0);
        let max_policy = maxes
            .iter()
            .copied()
            .max()
            .map(|v| v as f64 / 1000.0)
            .unwrap_or(0.0);

        let memory = read_memory_info(&self.mem_hw);

        let dt = int.disk_ts.map(|ts| (now - ts).max(1e-3)).unwrap_or(1.0);
        let mut disk_rows = Vec::new();
        let mut disk_prev_next = HashMap::new();
        for line in read_text("/proc/diskstats").lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 14 {
                continue;
            }
            let name = f[2];
            if name.starts_with("loop")
                || name.starts_with("ram")
                || name.starts_with("zram")
                || name.starts_with("dm-")
                || name.starts_with("sr")
            {
                continue;
            }
            if !Path::new(&format!("/sys/block/{}", name)).is_dir() {
                continue;
            }
            let (Ok(reads), Ok(sectors_r), Ok(writes), Ok(sectors_w), Ok(io_ms)) = (
                f[3].parse::<u64>(),
                f[5].parse::<u64>(),
                f[7].parse::<u64>(),
                f[9].parse::<u64>(),
                f[12].parse::<u64>(),
            ) else {
                continue;
            };
            let cur = (reads, sectors_r, writes, sectors_w, io_ms);
            disk_prev_next.insert(name.to_string(), cur);
            let mut rd = 0.0;
            let mut wr = 0.0;
            let mut active = 0.0;
            let mut iops = 0.0;
            let mut resp = 0.0;
            if let Some(&old) = int.disk_prev.get(name) {
                rd = sectors_r.saturating_sub(old.1) as f64 * 512.0 / dt;
                wr = sectors_w.saturating_sub(old.3) as f64 * 512.0 / dt;
                let d_io = io_ms.saturating_sub(old.4) as f64;
                let d_ops = (reads.saturating_sub(old.0) + writes.saturating_sub(old.2)) as f64;
                active = (d_io / (dt * 1000.0) * 100.0).min(100.0);
                iops = d_ops / dt;
                resp = if d_ops > 0.0 { d_io / d_ops } else { 0.0 };
            }
            let size = read_int(&format!("/sys/block/{}/size", name))
                .unwrap_or(0)
                .max(0) as u64
                * 512;
            let model = disk_model(&mut int.disk_models, name);
            let temp = if name.starts_with("nvme") && int.hwmon.nvme.is_some() {
                read_temp(&mut int.hwmon, "nvme")
            } else {
                None
            };
            disk_rows.push(DiskInfo {
                name: name.to_string(),
                model,
                read: rd,
                write: wr,
                active,
                iops,
                response: resp,
                size,
                temp,
            });
        }
        int.disk_prev = disk_prev_next;
        int.disk_ts = Some(now);

        let mut net_prev_next = HashMap::new();
        let mut ifaces = Vec::new();
        for line in read_text("/proc/net/dev").lines().skip(2) {
            let Some((raw_name, rest)) = line.split_once(':') else {
                continue;
            };
            let name = raw_name.trim();
            let f: Vec<&str> = rest.split_whitespace().collect();
            if name == "lo" || f.len() < 16 {
                continue;
            }
            let (Ok(rx), Ok(rxp), Ok(tx), Ok(txp)) = (
                f[0].parse::<u64>(),
                f[1].parse::<u64>(),
                f[8].parse::<u64>(),
                f[9].parse::<u64>(),
            ) else {
                continue;
            };
            net_prev_next.insert(name.to_string(), (rx, tx));
            let (rxr, txr) = match int.net_if_prev.get(name) {
                Some(&(old_rx, old_tx)) => (
                    rx.saturating_sub(old_rx) as f64 / dt,
                    tx.saturating_sub(old_tx) as f64 / dt,
                ),
                None => (0.0, 0.0),
            };
            let wireless = Path::new(&format!("/sys/class/net/{}/wireless", name)).is_dir();
            let driver = fs::read_link(format!("/sys/class/net/{}/device/driver", name))
                .ok()
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()));
            ifaces.push(NetIfaceInfo {
                name: name.to_string(),
                rx: rxr,
                tx: txr,
                rx_total: rx,
                tx_total: tx,
                rx_packets: rxp,
                tx_packets: txp,
                mac: read_text(&format!("/sys/class/net/{}/address", name))
                    .trim()
                    .to_string(),
                mtu: read_int(&format!("/sys/class/net/{}/mtu", name)),
                state: read_text(&format!("/sys/class/net/{}/operstate", name))
                    .trim()
                    .to_string(),
                ipv4: iface_ipv4(name),
                ipv6: iface_ipv6(name),
                wireless,
                driver,
            });
        }
        int.net_if_prev = net_prev_next;

        let wifi = ifaces.iter().find(|i| i.wireless).map(|wl| {
            let mut w = WifiInfo {
                iface: wl.name.clone(),
                driver: wl.driver.clone(),
                mac: wl.mac.clone(),
                ipv4: wl.ipv4.clone(),
                ipv6: wl.ipv6.clone(),
                temp: read_temp(&mut int.hwmon, "wifi"),
                ..Default::default()
            };
            for line in read_text("/proc/net/wireless").lines().skip(2) {
                let Some((name, rest)) = line.split_once(':') else {
                    continue;
                };
                if name.trim() != wl.name {
                    continue;
                }
                let cleaned = rest.replace('.', " ");
                let f: Vec<&str> = cleaned.split_whitespace().collect();
                if f.len() >= 3 {
                    if let (Ok(q), Ok(lvl)) = (f[1].parse::<f64>(), f[2].parse::<f64>()) {
                        w.signal_pct = Some((q / 70.0 * 100.0).clamp(0.0, 100.0));
                        w.signal_dbm = Some(lvl);
                    }
                }
            }
            if let Some(text) = run_cmd("iwconfig", &[&wl.name]) {
                w.ssid = extract_between(&text, "ESSID:\"", "\"");
                if let Some(freq_s) = extract_token_after(&text, "Frequency:") {
                    if let Ok(ghz) = freq_s.parse::<f64>() {
                        w.freq_ghz = Some(ghz);
                        w.channel = freq_to_channel(ghz);
                    }
                }
                if let Some(ap) = extract_token_after(&text, "Access Point: ") {
                    if ap.len() == 17 && ap.contains(':') {
                        w.ap = Some(ap);
                    }
                }
                if let Some(rate_s) = extract_token_after(&text, "Bit Rate=") {
                    if let Ok(mbps) = rate_s.parse::<f64>() {
                        w.link_mbps = Some(mbps);
                    }
                }
                if let Some(tx_s) = extract_token_after(&text, "Tx-Power=") {
                    if let Ok(dbm) = tx_s.parse::<i64>() {
                        w.txpower = Some(dbm);
                    }
                }
                w.powersave = extract_token_after(&text, "Power Management:");
            }
            w
        });

        let cpu_temp = read_temp(&mut int.hwmon, "cpu");
        let uptime = read_text("/proc/uptime")
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let load = read_loadavg();
        let no_turbo = read_int("/sys/devices/system/cpu/intel_pstate/no_turbo");
        let volumes = Self::volumes();

        let (gpu_total_now, totals_now) = {
            let sh = self.shared.lock().unwrap();
            (sh.gpu_total, sh.totals.clone())
        };

        let gpu = self.gpu_card.as_ref().map(|card| {
            let driver_link = Path::new(card).join("device/driver");
            let driver = fs::read_link(&driver_link)
                .ok()
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "drm".to_string());
            let freq = read_int(&format!("{}/gt/gt0/rps_cur_freq_mhz", card))
                .or_else(|| read_int(&format!("{}/gt_cur_freq_mhz", card)));
            let freq_act = read_int(&format!("{}/gt_act_freq_mhz", card))
                .or_else(|| read_int(&format!("{}/gt/gt0/rps_act_freq_mhz", card)));
            let freq_max = read_int(&format!("{}/gt_max_freq_mhz", card))
                .or_else(|| read_int(&format!("{}/gt/gt0/rps_max_freq_mhz", card)));
            GpuInfo {
                name: self.gpu_name.clone().unwrap_or_else(|| "GPU".to_string()),
                util: gpu_total_now,
                freq,
                freq_act,
                freq_max,
                driver,
            }
        });

        let sys_info = SystemInfo {
            cpu: CpuInfo {
                model: self.cpu_model.clone(),
                util,
                per_core: per_core.clone(),
                freq: freq_avg,
                freqs,
                base_mhz: base_p,
                base_e_mhz: base_e,
                max_policy_mhz: max_policy,
                cores: self.phys_cores,
                logical: self.ncpu,
                sockets: self.sockets,
                no_turbo,
                virtualization: self.cpu_flags.contains("vmx") || self.cpu_flags.contains("svm"),
                temp: cpu_temp,
                load,
            },
            memory: memory.clone(),
            disks: disk_rows.clone(),
            volumes,
            net: NetInfo {
                interfaces: ifaces.clone(),
                wifi,
            },
            gpu,
            uptime,
            totals: totals_now,
        };

        let mut sh = self.shared.lock().unwrap();
        sh.system = Some(sys_info);
        sh.sys_ts = now;
        push_hist(&mut sh.hist.cpu, (util * 100.0).round() / 100.0);
        let rounded_cores: Vec<f64> = per_core
            .iter()
            .map(|x| (x * 100.0).round() / 100.0)
            .collect();
        sh.hist.cores.push(rounded_cores);
        if sh.hist.cores.len() > HISTORY_LEN {
            let excess = sh.hist.cores.len() - HISTORY_LEN;
            sh.hist.cores.drain(0..excess);
        }
        push_hist(&mut sh.hist.mem, (memory.percent * 100.0).round() / 100.0);
        let (dr, dw) = disk_rows
            .first()
            .map(|d| (d.read, d.write))
            .unwrap_or((0.0, 0.0));
        push_hist(&mut sh.hist.disk_r, (dr * 10.0).round() / 10.0);
        push_hist(&mut sh.hist.disk_w, (dw * 10.0).round() / 10.0);
        let wl0 = ifaces
            .iter()
            .find(|i| i.wireless)
            .or_else(|| ifaces.first());
        let (nrx, ntx) = wl0.map(|i| (i.rx, i.tx)).unwrap_or((0.0, 0.0));
        push_hist(&mut sh.hist.net_rx, (nrx * 10.0).round() / 10.0);
        push_hist(&mut sh.hist.net_tx, (ntx * 10.0).round() / 10.0);
        push_hist(&mut sh.hist.gpu, (gpu_total_now * 100.0).round() / 100.0);
        drop(sh);
        self.sample_seq.fetch_add(1, Ordering::Release);
    }

    pub fn volumes() -> Vec<VolumeInfo> {
        let mut rows = Vec::new();
        let mut seen = HashSet::new();
        for line in read_text("/proc/mounts").lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 3 {
                continue;
            }
            let dev = f[0];
            let fstype = f[2];
            if !matches!(
                fstype,
                "ext2"
                    | "ext3"
                    | "ext4"
                    | "btrfs"
                    | "xfs"
                    | "f2fs"
                    | "vfat"
                    | "ntfs"
                    | "ntfs3"
                    | "exfat"
            ) {
                continue;
            }
            let mount = unescape_octal(f[1]);
            if !seen.insert(dev.to_string()) {
                continue;
            }
            let Ok(c_mount) = CString::new(mount.as_str()) else {
                continue;
            };
            let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
            let rc = unsafe { libc::statvfs(c_mount.as_ptr(), &mut st) };
            if rc != 0 {
                continue;
            }
            let total = (st.f_blocks as u64).saturating_mul(st.f_frsize as u64);
            let free = (st.f_bavail as u64).saturating_mul(st.f_frsize as u64);
            let used = total.saturating_sub(free);
            let percent = if total > 0 {
                used as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            rows.push(VolumeInfo {
                device: dev.to_string(),
                mount,
                fstype: fstype.to_string(),
                total,
                free,
                used,
                percent,
            });
        }
        rows
    }

    fn read_proc_mem(pid: u32, page_size: u64, raw_rss: u64) -> (u64, u64) {
        if raw_rss == 0 {
            return (0, 0);
        }

        // Try /proc/{pid}/smaps_rollup (fastest and exact PSS/RSS: Linux 4.14+)
        let rollup_path = format!("/proc/{}/smaps_rollup", pid);
        if let Ok(mut f) = File::open(&rollup_path) {
            let mut buf = [0u8; 1024];
            if let Ok(n) = f.read(&mut buf) {
                let text = String::from_utf8_lossy(&buf[..n]);
                let mut rss = None;
                let mut pss = None;
                for line in text.lines() {
                    if let Some(rest) = line.strip_prefix("Rss:") {
                        if let Some(kb) = rest
                            .split_whitespace()
                            .next()
                            .and_then(|s| s.parse::<u64>().ok())
                        {
                            rss = Some(kb.saturating_mul(1024));
                        }
                    } else if let Some(rest) = line.strip_prefix("Pss:") {
                        if let Some(kb) = rest
                            .split_whitespace()
                            .next()
                            .and_then(|s| s.parse::<u64>().ok())
                        {
                            pss = Some(kb.saturating_mul(1024));
                        }
                    }
                    if rss.is_some() && pss.is_some() {
                        break;
                    }
                }
                if let Some(p) = pss {
                    return (p, rss.unwrap_or(raw_rss));
                }
            }
        }

        // Fallback: /proc/{pid}/statm (world-readable, available for all processes including root)
        let statm_path = format!("/proc/{}/statm", pid);
        if let Ok(statm) = fs::read_to_string(&statm_path) {
            let parts: Vec<&str> = statm.split_whitespace().collect();
            if parts.len() >= 3 {
                let res = parts[1]
                    .parse::<u64>()
                    .unwrap_or(0)
                    .saturating_mul(page_size);
                let shm = parts[2]
                    .parse::<u64>()
                    .unwrap_or(0)
                    .saturating_mul(page_size);
                let priv_mem = res.saturating_sub(shm);
                // Proportional estimate: private memory + half of shared memory
                let pss_est = if shm > 0 {
                    priv_mem.saturating_add(shm / 2)
                } else {
                    res
                };
                return (pss_est, res);
            }
        }

        (raw_rss, raw_rss)
    }

    fn proc_stat(&self, pid: u32) -> Option<ProcStat> {
        let mut raw = Vec::with_capacity(512);
        File::open(format!("/proc/{}/stat", pid))
            .ok()?
            .read_to_end(&mut raw)
            .ok()?;
        let lp = raw.iter().position(|&b| b == b'(')?;
        let rp = raw.iter().rposition(|&b| b == b')')?;
        if rp <= lp {
            return None;
        }
        let name = String::from_utf8_lossy(&raw[lp + 1..rp]).into_owned();
        let rest = std::str::from_utf8(raw.get(rp + 2..)?).ok()?;
        let f: Vec<&str> = rest.split_whitespace().collect();
        if f.len() < 22 {
            return None;
        }
        let raw_rss = f[21].parse::<u64>().ok()?.saturating_mul(self.page_size);
        let vsize = f[20].parse().ok()?;
        let (pss, rss) = Self::read_proc_mem(pid, self.page_size, raw_rss);
        Some(ProcStat {
            name,
            state: f[0].to_string(),
            ppid: f[1].parse().ok()?,
            utime: f[11].parse().ok()?,
            stime: f[12].parse().ok()?,
            priority: f[15].parse().ok()?,
            nice: f[16].parse().ok()?,
            threads: f[17].parse().ok()?,
            starttime: f[19].parse().ok()?,
            vsize,
            rss,
            pss,
        })
    }

    pub fn scan_gpu(&self, now: f64) {
        let mut engine_now: HashMap<u32, HashMap<String, u64>> = HashMap::new();
        let mut fdcount: HashMap<u32, u64> = HashMap::new();

        if let Ok(entries) = fs::read_dir("/proc") {
            for ent in entries.flatten() {
                let Ok(name) = ent.file_name().into_string() else {
                    continue;
                };
                let Ok(pid) = name.parse::<u32>() else {
                    continue;
                };
                let mut count = 0u64;
                let mut engines: HashMap<String, u64> = HashMap::new();
                if let Ok(fd_entries) = fs::read_dir(format!("/proc/{}/fdinfo", pid)) {
                    for fd_ent in fd_entries.flatten() {
                        count += 1;
                        let Ok(mut fh) = File::open(fd_ent.path()) else {
                            continue;
                        };
                        let mut data = Vec::new();
                        if fh.read_to_end(&mut data).is_err() {
                            continue;
                        }
                        if !data.windows(11).any(|w| w == b"drm-engine-") {
                            continue;
                        }
                        let text = String::from_utf8_lossy(&data);
                        for line in text.lines() {
                            if let Some(rest) = line.strip_prefix("drm-engine-") {
                                if let Some((eng, val)) = rest.split_once(':') {
                                    if let Some(ns_str) = val.split_whitespace().next() {
                                        if let Ok(ns) = ns_str.parse::<u64>() {
                                            let e = engines.entry(eng.to_string()).or_insert(0);
                                            if ns > *e {
                                                *e = ns;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                fdcount.insert(pid, count);
                if !engines.is_empty() {
                    engine_now.insert(pid, engines);
                }
            }
        }

        let mut int = self.proc_internals.lock().unwrap();
        let interval = if int.gpu_ts > 0.0 {
            (now - int.gpu_ts).max(1e-3)
        } else {
            0.0
        };
        let mut per_proc = HashMap::new();
        let mut total_pct = 0.0;
        if interval > 0.0 {
            let mut total_ns = 0u64;
            for (pid, engines) in &engine_now {
                let Some(old) = int.gpu_prev.get(pid) else {
                    continue;
                };
                let mut best_ns = 0u64;
                let mut best_eng: Option<&str> = None;
                for (eng, &ns) in engines {
                    if let Some(&old_ns) = old.get(eng) {
                        let d = ns.saturating_sub(old_ns);
                        if d > best_ns {
                            best_ns = d;
                            best_eng = Some(eng.as_str());
                        }
                    }
                }
                if best_ns > 0 {
                    let pct = (best_ns as f64 / (interval * 1e9) * 100.0).min(100.0);
                    let eng_label = best_eng.map(|e| engine_name(e).to_string());
                    per_proc.insert(*pid, ((pct * 10.0).round() / 10.0, eng_label));
                    total_ns = total_ns.saturating_add(best_ns);
                }
            }
            total_pct = (total_ns as f64 / (interval * 1e9) * 100.0).min(100.0);
        }
        int.gpu_prev = engine_now;
        int.gpu_ts = now;
        drop(int);

        let mut sh = self.shared.lock().unwrap();
        sh.gpu_proc = per_proc;
        sh.gpu_total = (total_pct * 10.0).round() / 10.0;
        sh.fdcount = fdcount;
    }

    fn scan_net(&self, now: f64) {
        let Some(stdout) = run_cmd("ss", &["-tunpi"]) else {
            return;
        };

        let mut int = self.proc_internals.lock().unwrap();
        let interval = if int.net_ts > 0.0 {
            (now - int.net_ts).max(1e-3)
        } else {
            0.0
        };
        let mut cur = HashMap::new();
        let mut up: HashMap<u32, f64> = HashMap::new();
        let mut down: HashMap<u32, f64> = HashMap::new();
        let mut meta: Option<((u32, u32, String, String, String), u32)> = None;

        for line in stdout.lines() {
            if line.is_empty() || line.starts_with("Netid") {
                continue;
            }
            if line.starts_with(' ') || line.starts_with('\t') {
                let Some((key, pid)) = meta.take() else {
                    continue;
                };
                let sent = extract_u64_after(line, "bytes_sent:");
                let recv = extract_u64_after(line, "bytes_received:");
                if sent.is_none() && recv.is_none() {
                    continue;
                }
                let sent_v = sent.unwrap_or(0);
                let recv_v = recv.unwrap_or(0);
                if let Some(&(old_pid, old_s, old_r)) = int.prev_net.get(&key) {
                    if old_pid == pid && interval > 0.0 {
                        let du = sent_v.saturating_sub(old_s) as f64;
                        let dd = recv_v.saturating_sub(old_r) as f64;
                        *up.entry(pid).or_insert(0.0) += du / interval;
                        *down.entry(pid).or_insert(0.0) += dd / interval;
                    }
                }
                cur.insert(key, (pid, sent_v, recv_v));
                continue;
            }
            meta = None;
            if let Some((pid, fd)) = parse_ss_user(line) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    let key = (
                        pid,
                        fd,
                        parts[0].to_string(),
                        parts[4].to_string(),
                        parts[5].to_string(),
                    );
                    meta = Some((key, pid));
                }
            }
        }
        int.prev_net = cur;
        int.net_ts = now;
        drop(int);

        let mut sh = self.shared.lock().unwrap();
        let mut rates = HashMap::new();
        let all_pids: HashSet<u32> = up.keys().chain(down.keys()).copied().collect();
        for pid in all_pids {
            let u_rate = up.get(&pid).copied().unwrap_or(0.0);
            let d_rate = down.get(&pid).copied().unwrap_or(0.0);
            rates.insert(pid, (u_rate as u64, d_rate as u64));
            if let Some(app) = sh.pid_app.get(&pid).cloned() {
                let tot = sh.app_totals.entry(app).or_default();
                tot.up = tot.up.saturating_add((u_rate * interval) as u64);
                tot.down = tot.down.saturating_add((d_rate * interval) as u64);
            }
        }
        sh.net_proc = rates;
    }

    pub fn sample_processes(&self) {
        let now = now_secs();

        let (prev_proc_snap, dt, scan_count, mut meta_cache, mut uid_names) = {
            let mut int = self.proc_internals.lock().unwrap();
            let sc = int.scan_count;
            int.scan_count += 1;
            let meta = std::mem::take(&mut int.meta);
            let uids = std::mem::take(&mut int.uid_names);
            (int.prev_proc.clone(), 0.0, sc, meta, uids)
        };
        let _ = dt;

        let (proc_ts_prev, gpu_proc_snap, net_proc_snap, fdcount_snap) = {
            let sh = self.shared.lock().unwrap();
            (
                sh.proc_ts,
                sh.gpu_proc.clone(),
                sh.net_proc.clone(),
                sh.fdcount.clone(),
            )
        };
        let dt = if proc_ts_prev > 0.0 {
            (now - proc_ts_prev).max(1e-3)
        } else {
            0.0
        };

        let do_gpu = scan_count % 3 == 0;
        let do_net = scan_count % 2 == 0;

        let mut procs = Vec::with_capacity(512);
        let mut prev_proc_next = HashMap::with_capacity(512);
        let mut pid_app = HashMap::with_capacity(512);
        let mut app_delta: HashMap<String, (f64, u64, u64)> = HashMap::new();
        let mut threads_total = 0u64;

        if let Ok(entries) = fs::read_dir("/proc") {
            for ent in entries.flatten() {
                let Ok(name) = ent.file_name().into_string() else {
                    continue;
                };
                let Ok(pid) = name.parse::<u32>() else {
                    continue;
                };
                let Some(st) = self.proc_stat(pid) else {
                    continue;
                };

                let meta =
                    get_or_update_meta(&mut meta_cache, &mut uid_names, pid, st.starttime, now);
                let ticks = st.utime.saturating_add(st.stime);
                let (rd, wr) = read_proc_io(pid);
                let app = meta.app.clone().unwrap_or_else(|| st.name.clone());

                let mut cpu = 0.0;
                let mut dr = 0.0;
                let mut dw = 0.0;
                if let Some(old) = prev_proc_snap.get(&pid) {
                    if old.starttime == st.starttime {
                        let d_ticks = ticks.saturating_sub(old.ticks) as f64;
                        if dt > 0.0 {
                            cpu = (d_ticks / self.clk_tck / dt / (self.ncpu.max(1) as f64) * 100.0)
                                .clamp(0.0, 100.0);
                            dr = rd.saturating_sub(old.rd) as f64 / dt;
                            dw = wr.saturating_sub(old.wr) as f64 / dt;
                        }
                        let acc = app_delta.entry(app.clone()).or_insert((0.0, 0, 0));
                        acc.0 += d_ticks / self.clk_tck;
                        acc.1 = acc.1.saturating_add(rd.saturating_sub(old.rd));
                        acc.2 = acc.2.saturating_add(wr.saturating_sub(old.wr));
                    }
                }

                prev_proc_next.insert(
                    pid,
                    PrevProc {
                        starttime: st.starttime,
                        ticks,
                        rd,
                        wr,
                    },
                );
                pid_app.insert(pid, app.clone());
                threads_total = threads_total.saturating_add(st.threads);

                let (gpu_pct, gpu_engine) = gpu_proc_snap.get(&pid).cloned().unwrap_or((0.0, None));
                let (net_up, net_down) = net_proc_snap.get(&pid).copied().unwrap_or((0, 0));
                let handles = fdcount_snap.get(&pid).copied().unwrap_or(0);

                procs.push(ProcessInfo {
                    pid,
                    ppid: st.ppid,
                    status: state_label(&st.state).to_string(),
                    state: st.state,
                    name: st.name,
                    app,
                    exe: meta.exe,
                    user: meta.user,
                    uid: meta.uid,
                    cpu: (cpu * 10.0).round() / 10.0,
                    mem: st.pss,
                    rss: st.rss,
                    vmem: st.vsize,
                    threads: st.threads,
                    handles,
                    nice: st.nice,
                    priority: st.priority,
                    started: self.boot_time + st.starttime as f64 / self.clk_tck,
                    disk_r: dr as u64,
                    disk_w: dw as u64,
                    net_up,
                    net_down,
                    gpu: gpu_pct,
                    gpu_engine,
                    cmdline: meta.cmdline,
                });
            }
        }

        {
            let mut sh = self.shared.lock().unwrap();
            for (app, (c, r, w)) in app_delta {
                let tot = sh.app_totals.entry(app).or_default();
                tot.cpu_s += c;
                tot.disk_r = tot.disk_r.saturating_add(r);
                tot.disk_w = tot.disk_w.saturating_add(w);
            }
            sh.pid_app = pid_app;
            let n_procs = procs.len();
            sh.proc_list = procs;
            sh.proc_ts = now;
            sh.proc_samples += 1;
            let handles_total: u64 = sh.fdcount.values().sum();
            sh.totals = Totals {
                processes: n_procs,
                threads: threads_total,
                handles: handles_total,
            };
        }
        self.sample_seq.fetch_add(1, Ordering::Release);

        if meta_cache.len() > prev_proc_next.len().saturating_mul(2) {
            meta_cache.retain(|pid, _| prev_proc_next.contains_key(pid));
        }

        {
            let mut int = self.proc_internals.lock().unwrap();
            int.prev_proc = prev_proc_next;
            int.meta = meta_cache;
            int.uid_names = uid_names;
        }

        if do_gpu {
            self.scan_gpu(now);
        }
        if do_net {
            self.scan_net(now);
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let sessions = self.sessions_data();
        let sh = self.shared.lock().unwrap();
        let apps = build_apps(&sh.proc_list, &sh.app_totals);
        let users = build_users(&sh.proc_list, &sessions);
        Snapshot {
            ts: now_secs(),
            started: self.start_ts,
            sys_ts: sh.sys_ts,
            proc_ts: sh.proc_ts,
            proc_samples: sh.proc_samples,
            system: sh.system.clone(),
            history: sh.hist.clone(),
            processes: sh.proc_list.clone(),
            apps,
            users,
            totals: sh.totals.clone(),
        }
    }

    fn sessions_data(&self) -> Vec<SessionInfo> {
        let now = now_secs();
        {
            let sh = self.shared.lock().unwrap();
            if now - sh.sessions_ts < 15.0 && !sh.sessions_data.is_empty() {
                return sh.sessions_data.clone();
            }
        }
        let mut rows = Vec::new();
        if let Some(stdout) = run_cmd("loginctl", &["list-sessions", "--no-legend"]) {
            for line in stdout.lines() {
                let f: Vec<&str> = line.split_whitespace().collect();
                if f.len() < 6 {
                    continue;
                }
                let Ok(uid) = f[1].parse::<u32>() else {
                    continue;
                };
                rows.push(SessionInfo {
                    uid,
                    user: f[2].to_string(),
                    session: if f[3] != "-" { f[3] } else { "—" }.to_string(),
                    tty: if f[4] != "-" { f[4] } else { "—" }.to_string(),
                    state: f[5].to_string(),
                });
            }
        }
        let mut sh = self.shared.lock().unwrap();
        sh.sessions_ts = now;
        sh.sessions_data = rows.clone();
        rows
    }

    pub fn services(&self) -> Vec<ServiceRow> {
        let now = now_secs();
        {
            let sh = self.shared.lock().unwrap();
            if now - sh.services_ts < 10.0 && !sh.services_data.is_empty() {
                return sh.services_data.clone();
            }
        }
        let rows = collect_services();
        let mut sh = self.shared.lock().unwrap();
        sh.services_ts = now_secs();
        sh.services_data = rows.clone();
        rows
    }

    pub fn startup(&self) -> Vec<StartupRow> {
        let now = now_secs();
        {
            let sh = self.shared.lock().unwrap();
            if now - sh.startup_ts < 30.0 && !sh.startup_data.is_empty() {
                return sh.startup_data.clone();
            }
        }
        let rows = collect_startup();
        let mut sh = self.shared.lock().unwrap();
        sh.startup_ts = now_secs();
        sh.startup_data = rows.clone();
        rows
    }

    pub fn set_startup_enabled(&self, row: &StartupRow, enable: bool) -> Result<(), String> {
        match row.entry_type.as_str() {
            "Autostart" => {
                let user_dir = Path::new(&home_dir()).join(".config/autostart");
                fs::create_dir_all(&user_dir)
                    .map_err(|e| format!("Failed to create ~/.config/autostart: {}", e))?;
                let file_name = if !row.unit_or_file.is_empty() {
                    &row.unit_or_file
                } else {
                    Path::new(&row.source)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("app.desktop")
                };
                let user_path = user_dir.join(file_name);

                let content = if user_path.exists() {
                    fs::read_to_string(&user_path).unwrap_or_default()
                } else if !row.source.is_empty() && Path::new(&row.source).exists() {
                    fs::read_to_string(&row.source).unwrap_or_default()
                } else {
                    format!(
                        "[Desktop Entry]\nType=Application\nName={}\nExec={}\nComment={}\n",
                        row.name, row.command, row.description
                    )
                };

                let updated = update_desktop_autostart(&content, enable, &row.name, &row.command);
                fs::write(&user_path, updated)
                    .map_err(|e| format!("Failed to write {}: {}", user_path.display(), e))?;

                self.refresh_startup();
                Ok(())
            }
            "Systemd (user)" => {
                let unit = if !row.unit_or_file.is_empty() {
                    &row.unit_or_file
                } else {
                    &row.name
                };
                let action = if enable { "enable" } else { "disable" };
                let output = Command::new("systemctl")
                    .args(["--user", action, unit])
                    .output()
                    .map_err(|e| format!("Failed to run systemctl: {}", e))?;
                if !output.status.success() {
                    let err = String::from_utf8_lossy(&output.stderr);
                    return Err(format!("systemctl {} failed: {}", action, err.trim()));
                }
                self.refresh_startup();
                Ok(())
            }
            _ => Err("Unsupported startup entry type".to_string()),
        }
    }

    pub fn toggle_startup(&self, row: &StartupRow) -> Result<bool, String> {
        let currently_enabled = row.status.eq_ignore_ascii_case("enabled");
        let target = !currently_enabled;
        self.set_startup_enabled(row, target)?;
        Ok(target)
    }

    pub fn refresh_startup(&self) {
        let mut sh = self.shared.lock().unwrap();
        sh.startup_ts = 0.0;
        sh.startup_data.clear();
    }
}

pub fn terminate_pids(pids: &[u32]) -> Result<(), String> {
    signal_pids(pids, libc::SIGTERM, "end")
}

pub fn kill_pids(pids: &[u32]) -> Result<(), String> {
    signal_pids(pids, libc::SIGKILL, "kill")
}

fn signal_pids(pids: &[u32], signal: libc::c_int, verb: &str) -> Result<(), String> {
    let uid = my_uid();
    let mut errors = Vec::new();
    let mut attempted = 0usize;

    for &pid in pids {
        if pid <= 1 {
            continue;
        }
        let Ok(meta) = fs::metadata(format!("/proc/{}", pid)) else {
            continue;
        };
        use std::os::unix::fs::MetadataExt;
        let p_uid = meta.uid();
        if p_uid != uid && uid != 0 {
            errors.push(format!(
                "PID {}: You can only {} processes owned by your user.",
                pid, verb
            ));
            attempted += 1;
            continue;
        }
        attempted += 1;
        let rc = unsafe { libc::kill(pid as libc::pid_t, signal) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            errors.push(format!("Could not {} PID {}: {}", verb, pid, err));
        }
    }

    if !errors.is_empty() && errors.len() >= attempted {
        Err(errors.into_iter().take(5).collect::<Vec<_>>().join("\n"))
    } else {
        Ok(())
    }
}

fn build_apps(
    proc_list: &[ProcessInfo],
    app_totals: &HashMap<String, AppTotal>,
) -> Vec<AppHistoryRow> {
    let mut cur: HashMap<&str, (f64, u64, u64, u64, u64, u64, f64, usize)> = HashMap::new();
    for p in proc_list {
        let e = cur
            .entry(p.app.as_str())
            .or_insert((0.0, 0, 0, 0, 0, 0, 0.0, 0));
        e.0 += p.cpu;
        e.1 = e.1.saturating_add(p.mem);
        e.2 = e.2.saturating_add(p.disk_r);
        e.3 = e.3.saturating_add(p.disk_w);
        e.4 = e.4.saturating_add(p.net_up);
        e.5 = e.5.saturating_add(p.net_down);
        e.6 += p.gpu;
        e.7 += 1;
    }

    let mut rows: Vec<AppHistoryRow> = app_totals
        .iter()
        .map(|(app, tot)| {
            let c = cur.get(app.as_str()).copied().unwrap_or_default();
            AppHistoryRow {
                name: app.clone(),
                cpu: ((c.0 * 10.0).round() / 10.0).min(100.0),
                mem: c.1,
                gpu: ((c.6 * 10.0).round() / 10.0).min(100.0),
                procs: c.7,
                disk_r: c.2,
                disk_w: c.3,
                up: c.4,
                down: c.5,
                cpu_s: (tot.cpu_s * 10.0).round() / 10.0,
                total_disk_r: tot.disk_r,
                total_disk_w: tot.disk_w,
                total_up: tot.up,
                total_down: tot.down,
            }
        })
        .collect();

    rows.sort_by(|a, b| b.cpu_s.total_cmp(&a.cpu_s));
    rows.truncate(200);
    rows
}

fn build_users(proc_list: &[ProcessInfo], sessions: &[SessionInfo]) -> Vec<UserRow> {
    let mut by_uid: HashMap<u32, UserRow> = HashMap::new();
    for p in proc_list {
        let u = by_uid.entry(p.uid).or_insert_with(|| UserRow {
            uid: p.uid,
            user: p.user.clone(),
            session: "—".to_string(),
            tty: "—".to_string(),
            state: "—".to_string(),
            ..Default::default()
        });
        u.cpu += p.cpu;
        u.mem = u.mem.saturating_add(p.mem);
        u.disk = u.disk.saturating_add(p.disk_r).saturating_add(p.disk_w);
        u.up = u.up.saturating_add(p.net_up);
        u.down = u.down.saturating_add(p.net_down);
        u.procs += 1;
    }

    let mut allowed: HashSet<u32> = HashSet::from([my_uid()]);
    for s in sessions {
        allowed.insert(s.uid);
        let u = by_uid.entry(s.uid).or_insert_with(|| UserRow {
            uid: s.uid,
            user: s.user.clone(),
            ..Default::default()
        });
        u.session = s.session.clone();
        u.tty = s.tty.clone();
        u.state = s.state.clone();
    }

    let mut rows: Vec<UserRow> = by_uid
        .into_iter()
        .filter(|(uid, _)| allowed.contains(uid))
        .map(|(_, mut u)| {
            u.cpu = ((u.cpu * 10.0).round() / 10.0).min(100.0);
            if u.session.is_empty() {
                u.session = "—".to_string();
            }
            if u.tty.is_empty() {
                u.tty = "—".to_string();
            }
            if u.state.is_empty() {
                u.state = "—".to_string();
            }
            u
        })
        .collect();

    rows.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    rows
}

fn collect_services() -> Vec<ServiceRow> {
    let mut svc_pid: HashMap<String, u32> = HashMap::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for ent in entries.flatten() {
            let Ok(name) = ent.file_name().into_string() else {
                continue;
            };
            let Ok(pid) = name.parse::<u32>() else {
                continue;
            };
            let cg = read_text(&format!("/proc/{}/cgroup", pid));
            if let Some(unit) = cg
                .split('/')
                .filter(|seg| seg.ends_with(".service"))
                .map(|seg| seg.trim())
                .last()
            {
                let e = svc_pid.entry(unit.to_string()).or_insert(pid);
                if pid < *e {
                    *e = pid;
                }
            }
        }
    }

    let mut rows = Vec::new();
    for (scope, user) in [("system", false), ("user", true)] {
        let mut unit_args = Vec::new();
        let mut file_args = Vec::new();
        if user {
            unit_args.push("--user");
            file_args.push("--user");
        }
        unit_args.extend([
            "list-units",
            "--type=service",
            "--all",
            "--output=json",
            "--no-pager",
        ]);
        file_args.extend([
            "list-unit-files",
            "--type=service",
            "--output=json",
            "--no-pager",
        ]);

        let units: Vec<SystemctlUnit> = run_cmd("systemctl", &unit_args)
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let files: Vec<SystemctlUnitFile> = run_cmd("systemctl", &file_args)
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let states: HashMap<String, String> =
            files.into_iter().map(|f| (f.unit_file, f.state)).collect();

        for u in units {
            let startup = states.get(&u.unit).cloned().unwrap_or_default();
            let pid = svc_pid.get(&u.unit).copied();
            rows.push(ServiceRow {
                unit: u.unit,
                description: u.description,
                active: u.active,
                sub: u.sub,
                load: u.load,
                startup,
                scope: scope.to_string(),
                pid,
            });
        }
    }
    rows.sort_by(|a, b| a.scope.cmp(&b.scope).then_with(|| a.unit.cmp(&b.unit)));
    rows
}

pub fn update_desktop_autostart(
    content: &str,
    enable: bool,
    fallback_name: &str,
    fallback_exec: &str,
) -> String {
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    if lines.is_empty() || !lines.iter().any(|l| l.trim() == "[Desktop Entry]") {
        lines.insert(0, "[Desktop Entry]".to_string());
        lines.push("Type=Application".to_string());
        lines.push(format!("Name={}", fallback_name));
        if !fallback_exec.is_empty() {
            lines.push(format!("Exec={}", fallback_exec));
        }
    }

    let mut found_hidden = false;
    let mut found_gnome = false;
    let hidden_val = if enable {
        "Hidden=false"
    } else {
        "Hidden=true"
    };
    let gnome_val = if enable {
        "X-GNOME-Autostart-enabled=true"
    } else {
        "X-GNOME-Autostart-enabled=false"
    };

    for line in lines.iter_mut() {
        let trimmed = line.trim();
        if trimmed.starts_with("Hidden=") {
            *line = hidden_val.to_string();
            found_hidden = true;
        } else if trimmed.starts_with("X-GNOME-Autostart-enabled=") {
            *line = gnome_val.to_string();
            found_gnome = true;
        }
    }

    if !found_hidden {
        lines.push(hidden_val.to_string());
    }
    if !found_gnome {
        lines.push(gnome_val.to_string());
    }

    lines.join("\n") + "\n"
}

pub fn collect_startup_from_dirs(user_dir: &Path, sys_dirs: &[&Path]) -> Vec<StartupRow> {
    let mut rows = Vec::new();
    let mut autostart_files: HashMap<String, (std::path::PathBuf, bool)> = HashMap::new();

    // 1. Scan system autostart directories
    for sys_dir in sys_dirs {
        if let Ok(entries) = fs::read_dir(sys_dir) {
            for ent in entries.flatten() {
                if let Ok(name) = ent.file_name().into_string() {
                    if name.ends_with(".desktop") {
                        autostart_files.entry(name).or_insert((ent.path(), false));
                    }
                }
            }
        }
    }

    // 2. Scan user autostart directory (takes precedence over system)
    if let Ok(entries) = fs::read_dir(user_dir) {
        for ent in entries.flatten() {
            if let Ok(name) = ent.file_name().into_string() {
                if name.ends_with(".desktop") {
                    autostart_files.insert(name, (ent.path(), true));
                }
            }
        }
    }

    let mut names: Vec<String> = autostart_files.keys().cloned().collect();
    names.sort();

    for fn_name in names {
        let Some((path, is_user)) = autostart_files.get(&fn_name) else {
            continue;
        };
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let Some(d) = crate::desktop::parse_desktop_entry(&text) else {
            continue;
        };
        if d.get("Type").map(|s| s.as_str()).unwrap_or("Application") != "Application" {
            continue;
        }

        let hidden = d
            .get("Hidden")
            .map(|s| s.trim().eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let gnome_enabled = d
            .get("X-GNOME-Autostart-enabled")
            .map(|s| !s.trim().eq_ignore_ascii_case("false"))
            .unwrap_or(true);

        let enabled = !hidden && gnome_enabled;

        rows.push(StartupRow {
            name: d
                .get("Name")
                .cloned()
                .unwrap_or_else(|| fn_name.trim_end_matches(".desktop").to_string()),
            status: if enabled { "Enabled" } else { "Disabled" }.to_string(),
            entry_type: "Autostart".to_string(),
            scope: if *is_user { "user" } else { "system" }.to_string(),
            command: d.get("Exec").cloned().unwrap_or_default(),
            description: d.get("Comment").cloned().unwrap_or_default(),
            source: path.to_string_lossy().into_owned(),
            unit_or_file: fn_name,
        });
    }

    // 3. Scan systemd user service units (enabled and disabled)
    let unit_files: Vec<SystemctlUnitFile> = run_cmd(
        "systemctl",
        &[
            "--user",
            "list-unit-files",
            "--type=service",
            "--state=enabled,disabled",
            "--output=json",
            "--no-pager",
        ],
    )
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or_default();

    let units: Vec<SystemctlUnit> = run_cmd(
        "systemctl",
        &[
            "--user",
            "list-units",
            "--type=service",
            "--all",
            "--output=json",
            "--no-pager",
        ],
    )
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or_default();
    let desc: HashMap<String, String> =
        units.into_iter().map(|u| (u.unit, u.description)).collect();

    for u in unit_files {
        if !u.unit_file.ends_with(".service") {
            continue;
        }
        // Skip desktop-generated wrapper services (already handled above)
        if u.unit_file.starts_with("app-") && u.unit_file.contains("@autostart.service") {
            continue;
        }
        let description = desc.get(&u.unit_file).cloned().unwrap_or_default();
        let status = if u.state == "enabled" {
            "Enabled"
        } else {
            "Disabled"
        };
        rows.push(StartupRow {
            name: u.unit_file.clone(),
            status: status.to_string(),
            entry_type: "Systemd (user)".to_string(),
            scope: "user".to_string(),
            command: String::new(),
            description,
            source: String::new(),
            unit_or_file: u.unit_file,
        });
    }

    rows.sort_by(|a, b| {
        let a_sys = a.entry_type != "Autostart";
        let b_sys = b.entry_type != "Autostart";
        a_sys
            .cmp(&b_sys)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    rows
}

#[allow(dead_code)]
pub fn collect_startup_from_dir(autostart_dir: &Path) -> Vec<StartupRow> {
    collect_startup_from_dirs(autostart_dir, &[])
}

fn collect_startup() -> Vec<StartupRow> {
    let user_dir = Path::new(&home_dir()).join(".config/autostart");
    let sys_p1 = Path::new("/etc/xdg/autostart");
    let sys_p2 = Path::new("/usr/share/gnome/autostart");
    collect_startup_from_dirs(&user_dir, &[sys_p1, sys_p2])
}

fn get_or_update_meta(
    cache: &mut HashMap<u32, ProcMeta>,
    uid_names: &mut HashMap<u32, String>,
    pid: u32,
    starttime: u64,
    now: f64,
) -> ProcMeta {
    if let Some(m) = cache.get(&pid) {
        if m.starttime == starttime && now - m.ts < 10.0 {
            return m.clone();
        }
    }
    let uid = fs::metadata(format!("/proc/{}", pid))
        .map(|m| {
            use std::os::unix::fs::MetadataExt;
            m.uid()
        })
        .unwrap_or(0);
    let user = uid_name(uid_names, uid);
    let exe = fs::read_link(format!("/proc/{}/exe", pid))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let app = if !exe.is_empty() {
        Path::new(&exe)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    } else {
        None
    };
    let cmdline = read_cmdline(pid);
    let meta = ProcMeta {
        starttime,
        uid,
        user,
        exe,
        cmdline,
        app,
        ts: now,
    };
    cache.insert(pid, meta.clone());
    meta
}

fn read_cmdline(pid: u32) -> String {
    let Ok(bytes) = fs::read(format!("/proc/{}/cmdline", pid)) else {
        return String::new();
    };
    let mut s = String::with_capacity(bytes.len().min(400));
    for &b in bytes.iter().take(400) {
        if b == 0 {
            s.push(' ');
        } else if b.is_ascii() {
            s.push(b as char);
        } else {
            s.push('\u{FFFD}');
        }
    }
    s.trim().to_string()
}

fn read_proc_io(pid: u32) -> (u64, u64) {
    let Ok(f) = File::open(format!("/proc/{}/io", pid)) else {
        return (0, 0);
    };
    let mut rd = 0u64;
    let mut wr = 0u64;
    let reader = BufReader::new(f);
    for line in reader.lines().map_while(Result::ok) {
        if let Some(val) = line.strip_prefix("read_bytes:") {
            if let Ok(n) = val.trim().parse::<u64>() {
                rd = n;
            }
        } else if let Some(val) = line.strip_prefix("write_bytes:") {
            if let Ok(n) = val.trim().parse::<u64>() {
                wr = n;
            }
        }
    }
    (rd, wr)
}

fn uid_name(cache: &mut HashMap<u32, String>, uid: u32) -> String {
    if let Some(n) = cache.get(&uid) {
        return n.clone();
    }
    let name = unsafe {
        let pw = libc::getpwuid(uid as libc::uid_t);
        if pw.is_null() || (*pw).pw_name.is_null() {
            uid.to_string()
        } else {
            CStr::from_ptr((*pw).pw_name).to_string_lossy().into_owned()
        }
    };
    cache.insert(uid, name.clone());
    name
}

fn read_text(path: &str) -> String {
    fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

fn read_int(path: &str) -> Option<i64> {
    read_text(path).trim().parse::<i64>().ok()
}

fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd)
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

fn read_cpu_model() -> String {
    for line in read_text("/proc/cpuinfo").lines() {
        if line.starts_with("model name") {
            if let Some((_, v)) = line.split_once(':') {
                return v.trim().to_string();
            }
        }
    }
    "CPU".to_string()
}

fn read_physical_cores(ncpu: usize) -> usize {
    let mut ids = HashSet::new();
    if let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") {
        for ent in entries.flatten() {
            let Ok(name) = ent.file_name().into_string() else {
                continue;
            };
            if !name.starts_with("cpu") || !name[3..].chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let topo = ent.path().join("topology");
            if topo.is_dir() {
                let pkg =
                    read_int(&topo.join("physical_package_id").to_string_lossy()).unwrap_or(0);
                let core = read_int(&topo.join("core_id").to_string_lossy()).unwrap_or(0);
                ids.insert((pkg, core));
            }
        }
    }
    if ids.is_empty() { ncpu } else { ids.len() }
}

fn read_sockets() -> usize {
    let mut ids = HashSet::new();
    if let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") {
        for ent in entries.flatten() {
            let Ok(name) = ent.file_name().into_string() else {
                continue;
            };
            if !name.starts_with("cpu") || !name[3..].chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let path = ent.path().join("topology/physical_package_id");
            if path.exists() {
                ids.insert(read_int(&path.to_string_lossy()).unwrap_or(0));
            }
        }
    }
    ids.len().max(1)
}

fn read_cpu_flags() -> HashSet<String> {
    for line in read_text("/proc/cpuinfo").lines() {
        if line.starts_with("flags") {
            if let Some((_, v)) = line.split_once(':') {
                return v.split_whitespace().map(|s| s.to_string()).collect();
            }
        }
    }
    HashSet::new()
}

fn read_boot_time() -> f64 {
    for line in read_text("/proc/stat").lines() {
        if let Some(rest) = line.strip_prefix("btime ") {
            if let Ok(val) = rest.trim().parse::<f64>() {
                return val;
            }
        }
    }
    now_secs()
}

fn find_gpu() -> (Option<String>, Option<String>) {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return (None, None);
    };
    let mut cards: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("card") && n[4..].chars().all(|c| c.is_ascii_digit()))
        .collect();
    cards.sort();

    for card_name in cards {
        let card = format!("/sys/class/drm/{}", card_name);
        let dev_path = Path::new(&card).join("device");
        if !dev_path.is_dir() {
            continue;
        }
        let pci = fs::canonicalize(&dev_path)
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_default();
        let mut name = None;
        if !pci.is_empty() {
            if let Some(out) = run_cmd("lspci", &["-mm", "-s", &pci]) {
                let fields = parse_quoted_fields(&out);
                if fields.len() >= 3 && !fields[2].is_empty() {
                    name = Some(fields[2].clone());
                }
            }
        }
        let name = name.unwrap_or_else(|| {
            let vendor = read_text(&format!("{}/device/vendor", card))
                .trim()
                .to_lowercase();
            match vendor.as_str() {
                "0x8086" => "Intel Graphics",
                "0x1002" => "AMD Radeon Graphics",
                "0x10de" => "NVIDIA Graphics",
                _ => "GPU",
            }
            .to_string()
        });
        return (Some(name), Some(card));
    }
    (None, None)
}

fn parse_quoted_fields(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut inside = false;
    for ch in s.chars() {
        if ch == '"' {
            if inside {
                out.push(std::mem::take(&mut cur));
                inside = false;
            } else {
                inside = true;
            }
        } else if inside {
            cur.push(ch);
        }
    }
    out
}

fn refresh_hwmon(hwmon: &mut HwmonPaths) {
    let Ok(entries) = fs::read_dir("/sys/class/hwmon") else {
        return;
    };
    for ent in entries.flatten() {
        let p = ent.path();
        let name = read_text(&p.join("name").to_string_lossy())
            .trim()
            .to_string();
        let path_str = p.to_string_lossy().into_owned();
        if matches!(name.as_str(), "coretemp" | "k10temp" | "zenpower") {
            hwmon.cpu = Some(path_str);
        } else if name == "nvme" {
            hwmon.nvme = Some(path_str);
        } else if name.starts_with("iwlwifi") {
            hwmon.wifi = Some(path_str);
        }
    }
}

fn read_temp(hwmon: &mut HwmonPaths, kind: &str) -> Option<f64> {
    let path = match kind {
        "cpu" => hwmon.cpu.as_deref(),
        "nvme" => hwmon.nvme.as_deref(),
        "wifi" => hwmon.wifi.as_deref(),
        _ => None,
    };
    if let Some(dir) = path {
        if Path::new(dir).is_dir() {
            if kind == "cpu" {
                let mut best = None;
                if let Ok(entries) = fs::read_dir(dir) {
                    for ent in entries.flatten() {
                        let fname = ent.file_name().to_string_lossy().into_owned();
                        if fname.starts_with("temp") && fname.ends_with("_input") {
                            let inp = ent.path();
                            let lbl_path = inp.to_string_lossy().replace("_input", "_label");
                            let label = read_text(&lbl_path).trim().to_string();
                            if let Some(val) = read_int(&inp.to_string_lossy()) {
                                if label.starts_with("Package") || label.starts_with("Tctl") {
                                    return Some(val as f64 / 1000.0);
                                }
                                if best.is_none() {
                                    best = Some(val);
                                }
                            }
                        }
                    }
                }
                if let Some(v) = best {
                    return Some(v as f64 / 1000.0);
                }
            } else if let Some(val) = read_int(&format!("{}/temp1_input", dir)) {
                return Some(val as f64 / 1000.0);
            }
        }
    }
    refresh_hwmon(hwmon);
    None
}

fn disk_model(cache: &mut HashMap<String, String>, name: &str) -> String {
    if let Some(m) = cache.get(name) {
        return m.clone();
    }
    let mut model = read_text(&format!("/sys/block/{}/device/model", name))
        .trim()
        .to_string();
    if model.is_empty() && name.starts_with("nvme") && name.len() > 2 {
        model = read_text(&format!(
            "/sys/class/nvme/{}/model",
            &name[..name.len() - 2]
        ))
        .trim()
        .to_string();
    }
    let res = if model.is_empty() {
        name.to_string()
    } else {
        model
    };
    cache.insert(name.to_string(), res.clone());
    res
}

fn read_cpu_times() -> HashMap<String, (u64, u64)> {
    let mut data = HashMap::new();
    for line in read_text("/proc/stat").lines() {
        if !line.starts_with("cpu") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }
        let vals: Vec<u64> = parts[1..9].iter().filter_map(|s| s.parse().ok()).collect();
        if vals.len() == 8 {
            let total: u64 = vals.iter().sum();
            let idle = vals[3].saturating_add(vals[4]);
            data.insert(parts[0].to_string(), (total, idle));
        }
    }
    data
}

#[derive(Clone, Default, Debug)]
pub struct MemoryHardware {
    pub mem_type: Option<String>,
    pub speed_mts: Option<u64>,
    pub manufacturer: Option<String>,
    pub form_factor: Option<String>,
    pub slots_used: usize,
    pub slots_total: usize,
    pub installed_bytes: u64,
    pub max_capacity: Option<u64>,
}

pub fn parse_udev_memory(text: &str) -> Option<MemoryHardware> {
    let mut hw = MemoryHardware::default();
    let mut seen_indices: HashSet<usize> = HashSet::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("E:MEMORY_ARRAY_MAX_CAPACITY=") {
            if let Ok(cap) = rest.parse::<u64>() {
                if cap > 0 {
                    hw.max_capacity = Some(cap);
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("E:MEMORY_DEVICE_") {
            let Some((idx_str, key_val)) = rest.split_once('_') else {
                continue;
            };
            let Ok(idx) = idx_str.parse::<usize>() else {
                continue;
            };
            seen_indices.insert(idx);
            let Some((key, val)) = key_val.split_once('=') else {
                continue;
            };
            let val = val.trim();
            match key {
                "SIZE" => {
                    if let Ok(bytes) = val.parse::<u64>() {
                        if bytes > 0 {
                            hw.slots_used += 1;
                            hw.installed_bytes = hw.installed_bytes.saturating_add(bytes);
                        }
                    }
                }
                "TYPE" => {
                    if !val.is_empty()
                        && !val.eq_ignore_ascii_case("unknown")
                        && !val.eq_ignore_ascii_case("other")
                        && hw.mem_type.is_none()
                    {
                        hw.mem_type = Some(val.to_string());
                    }
                }
                "SPEED_MTS" | "CONFIGURED_SPEED_MTS" => {
                    if let Ok(spd) = val.parse::<u64>() {
                        if spd > 0 && hw.speed_mts.is_none() {
                            hw.speed_mts = Some(spd);
                        }
                    }
                }
                "MANUFACTURER" => {
                    if !val.is_empty()
                        && !val.eq_ignore_ascii_case("unknown")
                        && !val.eq_ignore_ascii_case("not specified")
                        && !val.eq_ignore_ascii_case("no dimm")
                        && hw.manufacturer.is_none()
                    {
                        hw.manufacturer = Some(val.to_string());
                    }
                }
                "FORM_FACTOR" => {
                    if !val.is_empty()
                        && !val.eq_ignore_ascii_case("unknown")
                        && hw.form_factor.is_none()
                    {
                        hw.form_factor = Some(val.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    if !seen_indices.is_empty() || hw.installed_bytes > 0 {
        hw.slots_total = seen_indices.len().max(hw.slots_used);
        Some(hw)
    } else {
        None
    }
}

pub fn read_edac_memory() -> Option<MemoryHardware> {
    let mut hw = MemoryHardware::default();
    let mut total_slots = 0;
    let mut used_slots = 0;
    let mut installed = 0u64;
    let mut detected_type = None;

    if let Ok(mc_entries) = fs::read_dir("/sys/devices/system/edac/mc") {
        for mc_ent in mc_entries.flatten() {
            let mc_path = mc_ent.path();
            if !mc_path.is_dir() {
                continue;
            }
            if let Ok(dimm_entries) = fs::read_dir(&mc_path) {
                for dimm_ent in dimm_entries.flatten() {
                    let dpath = dimm_ent.path();
                    let name = dimm_ent.file_name().to_string_lossy().to_string();
                    if !name.starts_with("dimm") {
                        continue;
                    }
                    total_slots += 1;
                    if let Ok(size_str) = fs::read_to_string(dpath.join("size")) {
                        if let Ok(mb) = size_str.trim().parse::<u64>() {
                            if mb > 0 {
                                used_slots += 1;
                                installed = installed.saturating_add(mb.saturating_mul(1024 * 1024));
                            }
                        }
                    }
                    if detected_type.is_none() {
                        if let Ok(t_str) = fs::read_to_string(dpath.join("dimm_mem_type")) {
                            let t = t_str.trim();
                            if !t.is_empty() && !t.eq_ignore_ascii_case("unknown") {
                                let normalized = match t {
                                    "Low-Power-DDR3-RAM" | "Low-Power-DDR3" => "LPDDR3",
                                    "Low-Power-DDR4-RAM" | "Low-Power-DDR4" => "LPDDR4",
                                    "Low-Power-DDR5-RAM" | "Low-Power-DDR5" => "LPDDR5",
                                    "Low-Power-DDR" => "LPDDR",
                                    "DDR5-RAM" => "DDR5",
                                    "DDR4-RAM" => "DDR4",
                                    "DDR3-RAM" => "DDR3",
                                    other => other,
                                };
                                detected_type = Some(normalized.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    if total_slots > 0 || installed > 0 {
        if let Ok(entries) = fs::read_dir("/sys/firmware/dmi/entries") {
            let dmi_17_count = entries
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("17-"))
                .count();
            if dmi_17_count > 0 {
                total_slots = dmi_17_count;
            }
        }
        hw.slots_used = used_slots;
        hw.slots_total = total_slots.max(used_slots);
        hw.installed_bytes = installed;
        if let Some(ref t) = detected_type {
            if t.starts_with("LPDDR") {
                hw.form_factor = Some("Row of chips".to_string());
            } else {
                hw.form_factor = Some("DIMM / SODIMM".to_string());
            }
        }
        hw.mem_type = detected_type;
        Some(hw)
    } else {
        None
    }
}

pub fn parse_dmidecode_memory(text: &str) -> Option<MemoryHardware> {
    let mut hw = MemoryHardware::default();
    let mut total_slots = 0;
    let mut used_slots = 0;
    let mut installed = 0u64;

    for block in text.split("Memory Device") {
        if !block.contains("Size:") {
            continue;
        }
        let lines: Vec<&str> = block.lines().collect();
        total_slots += 1;
        let mut block_size = 0u64;
        let mut block_type = None;
        let mut block_speed = None;
        let mut block_mfr = None;
        let mut block_form = None;

        for line in lines {
            let trimmed = line.trim();
            if let Some((k, v)) = trimmed.split_once(':') {
                let k = k.trim();
                let v = v.trim();
                match k {
                    "Size" => {
                        if !v.eq_ignore_ascii_case("No Module Installed") {
                            if let Some(num_str) = v.split_whitespace().next() {
                                if let Ok(val) = num_str.parse::<u64>() {
                                    if v.contains("GB") {
                                        block_size = val.saturating_mul(1024 * 1024 * 1024);
                                    } else if v.contains("MB") {
                                        block_size = val.saturating_mul(1024 * 1024);
                                    }
                                }
                            }
                        }
                    }
                    "Type" => {
                        if !v.eq_ignore_ascii_case("Unknown") && !v.eq_ignore_ascii_case("Other") {
                            block_type = Some(v.to_string());
                        }
                    }
                    "Speed" | "Configured Memory Speed" => {
                        if let Some(num_str) = v.split_whitespace().next() {
                            if let Ok(spd) = num_str.parse::<u64>() {
                                if spd > 0 {
                                    block_speed = Some(spd);
                                }
                            }
                        }
                    }
                    "Manufacturer" => {
                        if !v.eq_ignore_ascii_case("Unknown")
                            && !v.eq_ignore_ascii_case("Not Specified")
                            && !v.eq_ignore_ascii_case("NO DIMM")
                            && !v.is_empty()
                        {
                            block_mfr = Some(v.to_string());
                        }
                    }
                    "Form Factor" => {
                        if !v.eq_ignore_ascii_case("Unknown") && !v.is_empty() {
                            block_form = Some(v.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        if block_size > 0 {
            used_slots += 1;
            installed = installed.saturating_add(block_size);
            if hw.mem_type.is_none() {
                hw.mem_type = block_type;
            }
            if hw.speed_mts.is_none() {
                hw.speed_mts = block_speed;
            }
            if hw.manufacturer.is_none() {
                hw.manufacturer = block_mfr;
            }
            if hw.form_factor.is_none() {
                hw.form_factor = block_form;
            }
        }
    }

    if total_slots > 0 {
        hw.slots_used = used_slots;
        hw.slots_total = total_slots;
        hw.installed_bytes = installed;
        Some(hw)
    } else {
        None
    }
}

pub fn read_memory_hardware() -> MemoryHardware {
    if let Ok(data) = fs::read_to_string("/run/udev/data/+dmi:id") {
        if let Some(hw) = parse_udev_memory(&data) {
            return hw;
        }
    }
    if let Some(out) = run_cmd("udevadm", &["info", "--export-db"]) {
        if let Some(hw) = parse_udev_memory(&out) {
            return hw;
        }
    }
    if let Some(hw) = read_edac_memory() {
        return hw;
    }
    if let Some(out) = run_cmd("dmidecode", &["-t", "17"])
        .or_else(|| run_cmd("sudo", &["-n", "dmidecode", "-t", "17"]))
    {
        if let Some(hw) = parse_dmidecode_memory(&out) {
            return hw;
        }
    }
    MemoryHardware::default()
}

fn read_memory_info(hw: &MemoryHardware) -> MemoryInfo {
    let mut m: HashMap<String, u64> = HashMap::new();
    for line in read_text("/proc/meminfo").lines() {
        if let Some((k, rest)) = line.split_once(':') {
            if let Some(num) = rest.split_whitespace().next() {
                if let Ok(kb) = num.parse::<u64>() {
                    m.insert(k.to_string(), kb.saturating_mul(1024));
                }
            }
        }
    }
    let total = m.get("MemTotal").copied().unwrap_or(0);
    let free = m.get("MemFree").copied().unwrap_or(0);
    let avail = m.get("MemAvailable").copied().unwrap_or(free);
    let used = total.saturating_sub(avail);
    let swap_total = m.get("SwapTotal").copied().unwrap_or(0);
    let swap_free = m.get("SwapFree").copied().unwrap_or(0);
    let hw_reserved = if hw.installed_bytes > total {
        hw.installed_bytes - total
    } else {
        0
    };
    MemoryInfo {
        total,
        used,
        available: avail,
        free,
        cached: m.get("Cached").copied().unwrap_or(0) + m.get("SReclaimable").copied().unwrap_or(0),
        buffers: m.get("Buffers").copied().unwrap_or(0),
        committed: m.get("Committed_AS").copied().unwrap_or(0),
        commit_limit: m.get("CommitLimit").copied().unwrap_or(0),
        slab: m.get("Slab").copied().unwrap_or(0),
        kernel: m.get("KernelStack").copied().unwrap_or(0)
            + m.get("PageTables").copied().unwrap_or(0),
        dirty: m.get("Dirty").copied().unwrap_or(0),
        writeback: m.get("Writeback").copied().unwrap_or(0),
        swap_total,
        swap_free,
        swap_used: swap_total.saturating_sub(swap_free),
        percent: if total > 0 {
            used as f64 / total as f64 * 100.0
        } else {
            0.0
        },
        speed_mts: hw.speed_mts,
        mem_type: hw.mem_type.clone(),
        manufacturer: hw.manufacturer.clone(),
        form_factor: hw.form_factor.clone(),
        slots_used: hw.slots_used,
        slots_total: hw.slots_total,
        hw_reserved,
        installed: hw.installed_bytes.max(total),
        max_capacity: hw.max_capacity,
    }
}

fn read_loadavg() -> [f64; 3] {
    let mut avg = [0.0f64; 3];
    let n = unsafe { libc::getloadavg(avg.as_mut_ptr(), 3) };
    if n == 3 { avg } else { [0.0, 0.0, 0.0] }
}

fn iface_ipv4(ifname: &str) -> Option<String> {
    let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut ifap) } != 0 {
        return None;
    }
    let mut found = None;
    let mut cur = ifap;
    while !cur.is_null() {
        let ifa = unsafe { &*cur };
        if !ifa.ifa_name.is_null() && !ifa.ifa_addr.is_null() {
            let name = unsafe { CStr::from_ptr(ifa.ifa_name) }.to_string_lossy();
            let family = unsafe { (*ifa.ifa_addr).sa_family } as i32;
            if name == ifname && family == libc::AF_INET {
                let sin = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in) };
                let ip = std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                found = Some(ip.to_string());
                break;
            }
        }
        cur = ifa.ifa_next;
    }
    unsafe { libc::freeifaddrs(ifap) };
    found
}

fn iface_ipv6(ifname: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in read_text("/proc/net/if_inet6").lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 6 && parts[5] == ifname && parts[0].len() == 32 {
            let mut bytes = [0u8; 16];
            let mut ok = true;
            for (i, chunk) in bytes.iter_mut().enumerate() {
                if let Ok(b) = u8::from_str_radix(&parts[0][i * 2..i * 2 + 2], 16) {
                    *chunk = b;
                } else {
                    ok = false;
                    break;
                }
            }
            if ok {
                out.push(std::net::Ipv6Addr::from(bytes).to_string());
            }
        }
    }
    out
}

fn push_hist(vec: &mut Vec<f64>, val: f64) {
    vec.push(val);
    if vec.len() > HISTORY_LEN {
        let excess = vec.len() - HISTORY_LEN;
        vec.drain(0..excess);
    }
}

fn extract_between(text: &str, start: &str, end: &str) -> Option<String> {
    let idx = text.find(start)?;
    let rest = &text[idx + start.len()..];
    let end_idx = rest.find(end)?;
    Some(rest[..end_idx].to_string())
}

fn extract_token_after(text: &str, prefix: &str) -> Option<String> {
    let idx = text.find(prefix)?;
    let rest = &text[idx + prefix.len()..];
    rest.split_whitespace().next().map(|s| s.to_string())
}

fn extract_u64_after(text: &str, prefix: &str) -> Option<u64> {
    let idx = text.find(prefix)?;
    let rest = &text[idx + prefix.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn parse_ss_user(line: &str) -> Option<(u32, u32)> {
    let idx = line.find("users:((\"")?;
    let rest = &line[idx..];
    let pid_idx = rest.find("pid=")?;
    let pid_rest = &rest[pid_idx + 4..];
    let pid_s: String = pid_rest
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let fd_idx = pid_rest.find("fd=")?;
    let fd_rest = &pid_rest[fd_idx + 3..];
    let fd_s: String = fd_rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    Some((pid_s.parse().ok()?, fd_s.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_system_and_processes() {
        let sampler = Sampler::new();
        sampler.sample_system();
        sampler.sample_processes();
        let snap = sampler.snapshot();
        let sys = snap.system.expect("system info should exist");
        assert!(sys.memory.total > 0);
        assert!(!snap.processes.is_empty());
        for p in &snap.processes {
            assert!(p.cpu >= 0.0 && p.cpu <= 100.0);
        }
    }

    #[test]
    fn test_cpu_clamping_and_normalization() {
        let ncpu = 4usize;
        let prev_ticks = 1000u64;
        let now_ticks = 1000 + 4 * 100 * 10; // 400% unnormalized
        let dt = 1.0f64;
        let cpu = ((now_ticks - prev_ticks) as f64 / 100.0 / dt / (ncpu.max(1) as f64) * 100.0)
            .clamp(0.0, 100.0);
        assert_eq!(cpu, 100.0);
    }

    #[test]
    fn test_volumes_no_octal_escapes() {
        let vols = Sampler::volumes();
        for v in vols {
            assert!(!v.mount.contains(r"\040"));
        }
    }

    #[test]
    fn test_startup_missing_autostart_dir_graceful() {
        let rows = collect_startup_from_dir(Path::new("/non/existent/autostart/path"));
        // Must not panic or abort systemd user service collection
        let _ = rows.len();
    }

    #[test]
    fn test_services_collection() {
        let sampler = Sampler::new();
        let svcs = sampler.services();
        if let Some(first) = svcs.first() {
            assert!(!first.unit.is_empty());
            assert!(first.scope == "system" || first.scope == "user");
        }
    }

    #[test]
    fn test_parse_ss_user() {
        let line =
            r#"tcp ESTAB 0 0 127.0.0.1:41803 47.253.66.40:22101 users:(("brave",pid=4045,fd=176))"#;
        assert_eq!(parse_ss_user(line), Some((4045, 176)));
    }

    #[test]
    fn test_update_desktop_autostart_enable_and_disable() {
        let initial = "[Desktop Entry]\nType=Application\nName=MyApp\nExec=/usr/bin/myapp\n";
        // Disabling
        let disabled = update_desktop_autostart(initial, false, "MyApp", "/usr/bin/myapp");
        assert!(disabled.contains("Hidden=true"));
        assert!(disabled.contains("X-GNOME-Autostart-enabled=false"));

        // Re-enabling
        let enabled = update_desktop_autostart(&disabled, true, "MyApp", "/usr/bin/myapp");
        assert!(enabled.contains("Hidden=false"));
        assert!(enabled.contains("X-GNOME-Autostart-enabled=true"));
        assert!(!enabled.contains("Hidden=true"));
        assert!(!enabled.contains("X-GNOME-Autostart-enabled=false"));
    }

    #[test]
    fn test_read_proc_mem_proportional_and_rss() {
        let pid = std::process::id();
        let (pss, rss) = Sampler::read_proc_mem(pid, 4096, 4096);
        assert!(pss > 0);
        assert!(rss >= pss || rss > 0);
        // Zero RSS returns (0, 0)
        assert_eq!(Sampler::read_proc_mem(pid, 4096, 0), (0, 0));
    }

    #[test]
    fn test_parse_udev_memory() {
        let sample = r#"
E:MEMORY_ARRAY_MAX_CAPACITY=68719476736
E:MEMORY_DEVICE_0_SIZE=8589934592
E:MEMORY_DEVICE_0_TYPE=LPDDR5
E:MEMORY_DEVICE_0_SPEED_MTS=6400
E:MEMORY_DEVICE_0_MANUFACTURER=Kingston
E:MEMORY_DEVICE_0_FORM_FACTOR=Row Of Chips
E:MEMORY_DEVICE_1_SIZE=8589934592
E:MEMORY_DEVICE_1_TYPE=LPDDR5
E:MEMORY_DEVICE_1_SPEED_MTS=6400
E:MEMORY_DEVICE_1_MANUFACTURER=Kingston
E:MEMORY_DEVICE_1_FORM_FACTOR=Row Of Chips
"#;
        let hw = parse_udev_memory(sample).expect("should parse udev memory");
        assert_eq!(hw.mem_type.as_deref(), Some("LPDDR5"));
        assert_eq!(hw.speed_mts, Some(6400));
        assert_eq!(hw.manufacturer.as_deref(), Some("Kingston"));
        assert_eq!(hw.form_factor.as_deref(), Some("Row Of Chips"));
        assert_eq!(hw.slots_used, 2);
        assert_eq!(hw.slots_total, 2);
        assert_eq!(hw.installed_bytes, 17179869184);
        assert_eq!(hw.max_capacity, Some(68719476736));
    }

    #[test]
    fn test_parse_dmidecode_memory() {
        let sample = r#"
# dmidecode 3.5
Handle 0x0017, DMI type 17, 92 bytes
Memory Device
	Array Handle: 0x0016
	Total Width: 64 bits
	Data Width: 64 bits
	Size: 16 GB
	Form Factor: SODIMM
	Type: DDR5
	Speed: 4800 MT/s
	Manufacturer: Kingston
	Configured Memory Speed: 4800 MT/s

Handle 0x0018, DMI type 17, 92 bytes
Memory Device
	Array Handle: 0x0016
	Size: No Module Installed
	Form Factor: SODIMM
	Type: Unknown
"#;
        let hw = parse_dmidecode_memory(sample).expect("should parse dmidecode");
        assert_eq!(hw.mem_type.as_deref(), Some("DDR5"));
        assert_eq!(hw.speed_mts, Some(4800));
        assert_eq!(hw.manufacturer.as_deref(), Some("Kingston"));
        assert_eq!(hw.form_factor.as_deref(), Some("SODIMM"));
        assert_eq!(hw.slots_used, 1);
        assert_eq!(hw.slots_total, 2);
        assert_eq!(hw.installed_bytes, 16 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_read_memory_hardware_live() {
        let hw = read_memory_hardware();
        if let Some(ref t) = hw.mem_type {
            assert!(!t.is_empty());
        }
        assert!(hw.slots_total > 0 || hw.installed_bytes > 0);
    }
}
