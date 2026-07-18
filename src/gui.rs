//! egui 仪表盘 — 桌面版 (eframe) 或 Web 版 (WASM)
//!
//! 显示 Hub 总览、服务器列表、探针数据。
//! 编译桌面版: `cargo run --features gui -- web`
//! 编译 Web 版: `cargo build --target wasm32-unknown-unknown --features gui`

#![cfg_attr(feature = "gui", allow(dead_code))]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 从 Hub API 拉取的数据快照
#[derive(Default, Clone)]
struct HubSnapshot {
    server_count: usize,
    player_count: usize,
    top_servers: Vec<(String, usize)>,
    probes: Vec<ProbeInfo>,
    error: Option<String>,
    loading: bool,
}

#[derive(Default, Clone)]
struct ProbeInfo {
    hostname: String,
    cpu_pct: f64,
    ram_used: u64,
    ram_total: u64,
    uptime_secs: u64,
}

/// 异步拉取 Hub 数据（在后台线程中调用）
async fn fetch_snapshot(hub_url: &str) -> HubSnapshot {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build().ok();
    let client = match client {
        Some(c) => c,
        None => return HubSnapshot { error: Some("Failed to create HTTP client".into()), ..Default::default() },
    };

    let base = hub_url.trim_end_matches('/');

    // Fetch journal and host probes in parallel
    let journal_fut = client.get(format!("{base}/journal")).send();
    let host_fut = client.get(format!("{base}/host-report")).send();

    let (journal_resp, host_resp) = tokio::join!(journal_fut, host_fut);

    let mut snap = HubSnapshot::default();

    // Journal
    match journal_resp {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(players) = body.get("players").and_then(|v| v.as_object()) {
                    snap.server_count = players.len();
                    snap.player_count = players.values()
                        .filter_map(|v| v.as_array())
                        .map(|a| a.len())
                        .sum();

                    let mut servers: Vec<(&String, usize)> = players.iter()
                        .map(|(k, v)| (k, v.as_array().map_or(0, |a| a.len())))
                        .collect();
                    servers.sort_by(|a, b| b.1.cmp(&a.1));
                    snap.top_servers = servers.iter().take(10)
                        .map(|(k, v)| (k.to_string(), *v))
                        .collect();
                }
            }
        }
        Ok(resp) => snap.error = Some(format!("Journal API: {}", resp.status())),
        Err(e) => snap.error = Some(format!("Journal fetch: {e}")),
    }

    // Host probes
    match host_resp {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(arr) = body.as_array() {
                    for probe in arr.iter() {
                        snap.probes.push(ProbeInfo {
                            hostname: probe.get("hostname").and_then(|v| v.as_str()).unwrap_or("?").into(),
                            cpu_pct: probe.get("cpu_pct").and_then(|v| v.as_f64()).unwrap_or(0.0),
                            ram_used: probe.get("ram_used_bytes").and_then(|v| v.as_u64()).unwrap_or(0),
                            ram_total: probe.get("ram_total_bytes").and_then(|v| v.as_u64()).unwrap_or(1),
                            uptime_secs: probe.get("uptime_secs").and_then(|v| v.as_u64()).unwrap_or(0),
                        });
                    }
                }
            }
        }
        Ok(resp) => { snap.error = Some(format!("Host API: {}", resp.status())); }
        Err(e) => { snap.error = Some(format!("Host fetch: {e}")); }
    }

    snap
}

// ---------------------------------------------------------------------------
// eframe 桌面应用
// ---------------------------------------------------------------------------

#[cfg(feature = "gui")]
mod desktop {
    use eframe::egui::{self, Color32, RichText, Vec2};
    use std::time::Instant;
    use tokio::runtime::Runtime;

    use super::*;

    struct CreeperGui {
        hub_url: String,
        snapshot: HubSnapshot,
        last_refresh: Instant,
        refresh_interval: f64,
        runtime: Arc<Runtime>,
        running: Arc<AtomicBool>,
    }

    impl CreeperGui {
        fn new(hub_url: &str, runtime: Arc<Runtime>) -> Self {
            Self {
                hub_url: hub_url.to_string(),
                snapshot: HubSnapshot::default(),
                last_refresh: Instant::now(),
                refresh_interval: 10.0,
                runtime,
                running: Arc::new(AtomicBool::new(true)),
            }
        }

        fn refresh(&mut self) {
            let url = self.hub_url.clone();
            let rt = self.runtime.clone();
            let snap = rt.block_on(async { fetch_snapshot(&url).await });
            self.snapshot = snap;
        }
    }

    impl eframe::App for CreeperGui {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            // Auto-refresh
            if self.last_refresh.elapsed().as_secs_f64() >= self.refresh_interval {
                self.last_refresh = Instant::now();
                self.refresh();
                ctx.request_repaint();
            }

            // ── Top panel: Hub URL + refresh button ──
            egui::TopBottomPanel::top("top").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("Creeper").color(Color32::from_rgb(0, 200, 255)).strong());
                    ui.separator();
                    ui.label(RichText::new(format!("Hub: {}", self.hub_url)).size(14.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("↻ Refresh").clicked() {
                            self.refresh();
                        }
                    });
                });
            });

            // ── Central panel ──
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(ref err) = self.snapshot.error {
                    ui.colored_label(Color32::RED, format!("⚠ {err}"));
                    ui.separator();
                }

                if self.snapshot.loading {
                    ui.label("Loading...");
                    return;
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    // ── Summary row ──
                    ui.horizontal(|ui| {
                        summary_card(ui, "Servers", &format!("{}", self.snapshot.server_count), Color32::from_rgb(100, 200, 255));
                        summary_card(ui, "Players", &format!("{}", self.snapshot.player_count), Color32::from_rgb(100, 255, 150));
                        summary_card(ui, "Probes", &format!("{}", self.snapshot.probes.len()), Color32::from_rgb(255, 200, 100));
                    });

                    ui.add_space(10.0);

                    // ── Top servers ──
                    if !self.snapshot.top_servers.is_empty() {
                        ui.heading("Top Servers");
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .max_height(300.0)
                            .show(ui, |ui| {
                                for (i, (srv, count)) in self.snapshot.top_servers.iter().enumerate() {
                                    let color = if *count >= 10 {
                                        Color32::GREEN
                                    } else if *count >= 4 {
                                        Color32::YELLOW
                                    } else {
                                        Color32::GRAY
                                    };
                                    ui.horizontal(|ui| {
                                        ui.label(format!("{}.", i + 1));
                                        ui.label(srv);
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.colored_label(color, format!("{count} players"));
                                        });
                                    });
                                }
                            });
                    }

                    ui.add_space(10.0);

                    // ── Host probes ──
                    if !self.snapshot.probes.is_empty() {
                        ui.heading("Host Probes");
                        ui.separator();
                        for probe in &self.snapshot.probes {
                            let ram_pct = probe.ram_used as f64 / probe.ram_total as f64 * 100.0;
                            let uptime_h = probe.uptime_secs as f64 / 3600.0;
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&probe.hostname).strong().size(14.0));
                                    ui.label(format!("CPU: {:.1}%", probe.cpu_pct));
                                    ui.label(format!("RAM: {:.0}%", ram_pct));
                                    ui.label(format!("Up: {:.0}h", uptime_h));
                                });
                                // CPU bar
                                let cpu = probe.cpu_pct as f32 / 100.0;
                                ui.add(egui::ProgressBar::new(cpu.clamp(0.0, 1.0))
                                    .text(format!("CPU {:.1}%", probe.cpu_pct))
                                    .fill(Color32::from_rgb(100, 200, 255)));
                                // RAM bar
                                let ram = probe.ram_used as f32 / probe.ram_total as f32;
                                ui.add(egui::ProgressBar::new(ram.clamp(0.0, 1.0))
                                    .text(format!("RAM {:.0}%", ram_pct))
                                    .fill(Color32::from_rgb(100, 255, 150)));
                            });
                        }
                    }
                });
            });
        }
    }

    fn summary_card(ui: &mut egui::Ui, title: &str, value: &str, color: Color32) {
        ui.group(|ui| {
            ui.set_min_size(Vec2::new(120.0, 60.0));
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(value).color(color).size(28.0).strong());
                ui.label(RichText::new(title).size(12.0));
            });
        });
    }

    /// 启动 egui 桌面窗口
    pub fn run_gui(hub_url: &str) -> Result<(), Box<dyn std::error::Error>> {
        let runtime = Arc::new(Runtime::new()?);
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("Creeper Dashboard")
                .with_inner_size([900.0, 680.0]),
            ..Default::default()
        };
        eframe::run_native(
            "Creeper",
            options,
            Box::new(|_cc| {
                Ok(Box::new(CreeperGui::new(hub_url, runtime.clone())))
            }),
        )?;
        Ok(())
    }
}

#[cfg(feature = "gui")]
pub use desktop::run_gui;