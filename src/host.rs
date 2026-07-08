//! # Host Monitor — 本机系统资源探针
//!
//! 循环采集 CPU / RAM / 磁盘 / 在线时长 / 负载 / 网络流量，
//! POST 到 hub 的 `/host-report` 端点聚合。仅供本机自有 hub 用。

use serde::{Deserialize, Serialize};
use sysinfo::{System, Disks, Networks};
use std::time::{SystemTime, UNIX_EPOCH};

/// 单次本机资源快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostReport {
    pub hostname: String,
    pub os_name: String,
    /// 系统启动至今秒数（uptime）
    pub uptime_secs: u64,
    /// 采集时间戳（UNIX 秒）
    pub timestamp: u64,
    /// CPU 总占用百分数（0..=100）
    pub cpu_pct: f32,
    /// RAM 已用 / 总量（字节）
    pub ram_used_bytes: u64,
    pub ram_total_bytes: u64,
    /// swap 已用 / 总量（字节）
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    /// 各挂载点磁盘用量
    pub disks: Vec<DiskReport>,
    /// 系统平均负载（1/5/15 分钟），仅 Unix 有意义
    pub load_avg_1: f64,
    pub load_avg_5: f64,
    pub load_avg_15: f64,
    /// 各网卡累计收发字节数
    pub nets: Vec<NetReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskReport {
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetReport {
    pub name: String,
    pub received_bytes: u64,
    pub transmitted_bytes: u64,
}

/// 采集一次本机快照。`sys` 需由调用方持有并定期 `refresh_cpu_usage()`。
pub fn snapshot(sys: &mut System) -> HostReport {
    sys.refresh_cpu_usage();
    // sysinfo 0.33：refresh_cpu_usage 后需让一帧间隔才能取到非 0 值；
    // 首次调用返回 0 是正常的，后续轮就有真实差值。
    sys.refresh_memory();

    let cpu_pct = sys.global_cpu_usage();
    let disks = Disks::new_with_refreshed_list();
    let nets = Networks::new_with_refreshed_list();

    let load = sysinfo::System::load_average();
    let uptime = System::uptime();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    HostReport {
        hostname: System::host_name().unwrap_or_else(|| "unknown".into()),
        os_name: System::name().unwrap_or_else(|| "unknown".into()),
        uptime_secs: uptime,
        timestamp: now,
        cpu_pct,
        ram_used_bytes: sys.used_memory(),
        ram_total_bytes: sys.total_memory(),
        swap_used_bytes: sys.used_swap(),
        swap_total_bytes: sys.total_swap(),
        disks: disks.iter().map(|d| DiskReport {
            mount_point: d.mount_point().to_string_lossy().to_string(),
            total_bytes: d.total_space(),
            available_bytes: d.available_space(),
        }).collect(),
        load_avg_1: load.one,
        load_avg_5: load.five,
        load_avg_15: load.fifteen,
        nets: nets.iter().map(|(name, data)| NetReport {
            name: name.clone(),
            received_bytes: data.received(),
            transmitted_bytes: data.transmitted(),
        }).collect(),
    }
}

/// 循环采集 + POST 到 hub。`interval_secs` 一轮，Ctrl+C 退出。
pub async fn monitor_loop(hub_url: &str, interval_secs: u64) -> anyhow::Result<()> {
    let mut sys = System::new_all();
    sys.refresh_cpu_usage(); // 首帧基线，下轮才有真实差值
    let client = reqwest::Client::new();
    let url = format!("{}/host-report", hub_url.trim_end_matches('/'));
    let interval = std::time::Duration::from_secs(interval_secs.max(1));

    log::info!("[host-monitor] start: hub={}, interval={}s", hub_url, interval_secs);
    println!("Host monitor running → {} every {}s. Ctrl+C to stop.", url, interval_secs);

    loop {
        let report = snapshot(&mut sys);
        match client.post(&url).json(&report).timeout(std::time::Duration::from_secs(10)).send().await {
            Ok(resp) if resp.status().is_success() => {
                log::debug!("[host-monitor] report sent: cpu={:.1}% ram={}/{}, {} disks",
                    report.cpu_pct, report.ram_used_bytes, report.ram_total_bytes, report.disks.len());
            }
            Ok(resp) => {
                log::warn!("[host-monitor] hub returned {}: {}", resp.status(), url);
            }
            Err(e) => {
                log::warn!("[host-monitor] failed to POST {}: {}", url, e);
            }
        }
        tokio::time::sleep(interval).await;
    }
}
