use std::collections::BTreeSet;
use std::fs;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use crate::discover::{self, DiscoveredServer};
use crate::motd;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingScan {
    description: String,
    softyun_scanned: ProviderGroup,
    z100_scanned: ProviderGroup,
    yxsjmc_n2_series: ProviderGroup,
    yxsjmc_n3_series: ProviderGroup,
    yxsjmc_n6_series: ProviderGroup,
    yxsjmc_other: ProviderGroup,
    deprecated: ProviderGroup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderGroup {
    provider: String,
    #[serde(default)]
    resolved: Vec<String>,
    notes: String,
    hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TargetsFile {
    description: String,
    targets: Vec<TargetEntry>,
    pending_scan: PendingScan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TargetEntry {
    provider: String,
    host: String,
    port: u16,
    interval_secs: u64,
}

pub struct CrawlConfig {
    pub targets_path: String,
    pub port_ranges: Vec<(u16, u16)>,
    pub concurrency: usize,
    pub delay_between_groups: Duration,
    pub delay_between_ports: Duration,
    pub save_on_discovery: bool,
}

impl Default for CrawlConfig {
    fn default() -> Self {
        Self {
            targets_path: "./data/mc-targets.json".to_string(),
            port_ranges: vec![(25565, 25565), (10000, 10500), (25000, 26000), (21000, 23000)],
            concurrency: 5,
            delay_between_groups: Duration::from_secs(300),
            delay_between_ports: Duration::from_secs(5),
            save_on_discovery: true,
        }
    }
}

fn load_targets_file(path: &str) -> Result<TargetsFile> {
    let data = fs::read_to_string(path)
        .context(format!("Failed to read targets file: {}", path))?;
    serde_json::from_str(&data).context("Failed to parse targets file")
}

fn save_targets_file(path: &str, file: &TargetsFile) -> Result<()> {
    let json = serde_json::to_string_pretty(file)?;
    fs::write(path, json)?;
    Ok(())
}

fn collect_all_hosts(pending: &PendingScan) -> Vec<(String, String)> {
    let mut all = Vec::new();
    all.extend(pending.softyun_scanned.hosts.iter().map(|h| (pending.softyun_scanned.provider.clone(), h.clone())));
    all.extend(pending.z100_scanned.hosts.iter().map(|h| (pending.z100_scanned.provider.clone(), h.clone())));
    all.extend(pending.yxsjmc_n2_series.hosts.iter().map(|h| (pending.yxsjmc_n2_series.provider.clone(), h.clone())));
    all.extend(pending.yxsjmc_n3_series.hosts.iter().map(|h| (pending.yxsjmc_n3_series.provider.clone(), h.clone())));
    all.extend(pending.yxsjmc_n6_series.hosts.iter().map(|h| (pending.yxsjmc_n6_series.provider.clone(), h.clone())));
    all.extend(pending.yxsjmc_other.hosts.iter().map(|h| (pending.yxsjmc_other.provider.clone(), h.clone())));
    all
}

fn port_ranges_to_list(ranges: &[(u16, u16)]) -> Vec<u16> {
    let mut ports = BTreeSet::new();
    for &(lo, hi) in ranges {
        for p in lo..=hi {
            ports.insert(p);
        }
    }
    ports.into_iter().collect()
}

pub async fn crawl(config: CrawlConfig) -> Result<()> {
    let mut targets_file = load_targets_file(&config.targets_path)?;
    let all_hosts = collect_all_hosts(&targets_file.pending_scan);
    
    log::info!("Crawl mode: {} hosts, {} port ranges, concurrency={}", 
        all_hosts.len(), config.port_ranges.len(), config.concurrency);
    
    let ports = port_ranges_to_list(&config.port_ranges);
    log::info!("Total ports to scan: {}", ports.len());
    
    let mut discovered_count = 0;
    let mut scan_round = 0;
    
    loop {
        scan_round += 1;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        log::info!("[Crawl Round {}] Starting at {}", scan_round, now);
        
        let mut results: Vec<(String, String, DiscoveredServer)> = Vec::new();
        
        for (provider, host) in &all_hosts {
            log::info!("[Round {}] Scanning {} (provider: {})", scan_round, host, provider);
            
            let patterns = vec![discover::DomainPattern {
                template: host.clone(),
                start: 0,
                end: 0,
            }];
            
            for chunk in ports.chunks(10) {
                let chunk_ports: Vec<u16> = chunk.to_vec();
                
                let found = discover::discover(&patterns, &chunk_ports, config.concurrency).await;
                
                for server in found {
                    log::info!("[DISCOVERED] {}:{} — {} {} {}/{}",
                        provider, server.host, server.port, server.game_version,
                        server.players_online, server.players_max);
                    results.push((provider.clone(), host.clone(), server));
                }
                
                sleep(config.delay_between_ports).await;
            }
            
            sleep(Duration::from_secs(10)).await;
        }
        
        if !results.is_empty() {
            discovered_count += results.len();
            
            if config.save_on_discovery {
                for (provider, host, server) in &results {
                    let exists = targets_file.targets.iter().any(
                        |t| t.host == server.host && t.port == server.port
                    );
                    
                    if !exists {
                        log::info!("[SAVE] Adding {}:{} to targets", server.host, server.port);
                        targets_file.targets.push(TargetEntry {
                            provider: provider.clone(),
                            host: server.host.clone(),
                            port: server.port,
                            interval_secs: 60,
                        });
                    }
                }
                
                targets_file.description = format!(
                    "MC 服务器监控目标，由 creeper monitor --targets 读取 | 最后扫描: {}, 爬取发现: {}",
                    chrono::Local::now().format("%Y-%m-%d"), discovered_count
                );
                
                save_targets_file(&config.targets_path, &targets_file)?;
                log::info!("[SAVE] Updated targets file: {} total targets", targets_file.targets.len());
            }
            
            for (provider, _host, server) in &results {
                motd::print_server_info(&crate::motd::PingResult {
                    info: motd::ServerInfo {
                        game_version: server.game_version.clone(),
                        protocol_version: -1,
                        edition: server.edition.clone(),
                        motd: server.motd.clone(),
                        players_online: server.players_online,
                        players_max: server.players_max,
                        players: server.players.clone(),
                        favicon: None,
                    },
                    protocol: if server.edition == "Java" {
                        crate::motd::Protocol::Java
                    } else {
                        crate::motd::Protocol::Bedrock
                    },
                    cached_at: None,
                });
            }
        } else {
            log::info!("[Round {}] No new servers discovered", scan_round);
        }
        
        log::info!("[Round {}] Complete. Waiting {}s before next round...",
            scan_round, config.delay_between_groups.as_secs());
        
        sleep(config.delay_between_groups).await;
    }
}

pub fn print_crawl_results(results: &[(String, String, DiscoveredServer)]) {
    if results.is_empty() {
        println!("No MC servers found.");
        return;
    }
    
    println!("\n=== Found {} MC server(s) ===\n", results.len());
    for (provider, _host, s) in results {
        println!(
            "  {:20} {:40} {:>6}  {:20}  {:>3}/{:>3}  {}",
            provider,
            s.host,
            s.port,
            s.game_version,
            s.players_online,
            s.players_max,
            if s.players.is_empty() {
                String::new()
            } else {
                format!("[{}]", s.players.join(", "))
            }
        );
    }
    println!();
}
