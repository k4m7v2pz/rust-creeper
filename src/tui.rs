//! Terminal UI (TUI) dashboard for Creeper.
//!
//! Monitors server status, player journal growth, and flood stats
//! in a live-updating terminal interface.

use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute, terminal};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use crate::motd;

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Monitor,
    Flood,
    Journal,
    Nodes,
    About,
}

const TABS: &[Tab] = &[Tab::Monitor, Tab::Flood, Tab::Journal, Tab::Nodes, Tab::About];

impl Tab {
    fn label(&self) -> &'static str {
        match self {
            Self::Monitor => " Monitor ",
            Self::Flood => " Flood ",
            Self::Journal => " Journal ",
            Self::Nodes => " Nodes ",
            Self::About => " About ",
        }
    }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

struct App {
    tab: Tab,
    should_quit: bool,
    /// Last MOTD ping result
    last_result: Option<motd::PingResult>,
    /// When the last ping was made
    last_ping: Instant,
    /// Ping interval (seconds)
    ping_interval: u64,
    /// Host:port being monitored
    host: String,
    port: u16,
    /// Log buffer
    logs: Vec<String>,
    /// Hub URL for node data
    hub_url: String,
    /// Cached node data (JSON string, refreshed periodically)
    node_data: String,
    /// Last node data refresh
    last_node_refresh: Instant,
    /// Node refresh interval (seconds)
    node_refresh_interval: u64,
}

impl App {
    fn new(hub_url: &str) -> Self {
        Self {
            tab: Tab::Nodes,
            should_quit: false,
            last_result: None,
            last_ping: Instant::now(),
            ping_interval: 0,
            host: String::new(),
            port: 0,
            logs: vec![],
            hub_url: hub_url.to_string(),
            node_data: String::new(),
            last_node_refresh: Instant::now(),
            node_refresh_interval: 10,
        }
    }

    fn tick(&mut self) {
        // Ping only when an interval is configured and a server is set
        if self.ping_interval > 0 && !self.host.is_empty() {
            if self.last_ping.elapsed() > Duration::from_secs(self.ping_interval) {
                self.last_ping = Instant::now();
                let host = self.host.clone();
                let port = self.port;
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        motd::ping(&host, port, -1, false)
                    })
                });

                if !result.info.motd.is_empty() {
                    let log = format!(
                        "{} {} {}/{}",
                        result.info.game_version,
                        result.info.motd,
                        result.info.players_online,
                        result.info.players_max,
                    );
                    self.logs.push(log);
                    if self.logs.len() > 100 {
                        self.logs.remove(0);
                    }
                }
                self.last_result = Some(result);
            }
        }

        // Refresh node data periodically
        if !self.hub_url.is_empty()
            && self.last_node_refresh.elapsed() > Duration::from_secs(self.node_refresh_interval)
        {
            self.last_node_refresh = Instant::now();
            let url = self.hub_url.clone();
            let result = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    fetch_node_data(&url).await
                })
            });
            self.node_data = result;
        }
    }
}

// ---------------------------------------------------------------------------
// UI render
// ---------------------------------------------------------------------------

fn ui(f: &mut Frame, app: &App) {
    let size = f.size();
    if size.height < 10 || size.width < 40 {
        return;
    }

    // ── Layout ──
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header + tabs
            Constraint::Min(2),     // body
            Constraint::Length(3),  // footer
        ])
        .split(size);

    render_header(f, chunks[0], app);
    render_body(f, chunks[1], app);
    render_footer(f, chunks[2]);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(2)])
        .split(area);

    // Title line
    let title = Line::from(Span::styled(
        format!(" Creeper v{} ", env!("CARGO_PKG_VERSION")),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    f.render_widget(title, chunks[0]);

    // Tabs as horizontal spans
    let tab_line = Line::from(
        TABS.iter().map(|t| {
            let selected = t == &app.tab;
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White).bg(Color::DarkGray)
            };
            Span::styled(format!(" {} ", t.label()), style)
        }).collect::<Vec<_>>()
    );
    f.render_widget(tab_line, chunks[1]);
}

fn render_body(f: &mut Frame, area: Rect, app: &App) {
    match app.tab {
        Tab::Monitor => render_monitor_tab(f, area, app),
        Tab::Flood => render_flood_tab(f, area, app),
        Tab::Journal => render_journal_tab(f, area, app),
        Tab::Nodes => render_nodes_tab(f, area, app),
        Tab::About => render_about_tab(f, area, app),
    }
}

fn render_monitor_tab(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),  // server info
            Constraint::Min(3),     // logs
        ])
        .split(area);

    // Server info panel
    let info_block = Block::default()
        .title(format!(" Server Info — {}:{} ", app.host, app.port))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let info_area = info_block.inner(chunks[0]);
    f.render_widget(info_block, chunks[0]);

    if let Some(ref result) = app.last_result {
        let info = &result.info;
        let mut lines = vec![
            Line::from(format!("Version : {} (protocol {})", info.game_version, info.protocol_version)),
            Line::from(format!("MOTD    : {}", info.motd)),
            Line::from(format!("Players : {}/{}", info.players_online, info.players_max)),
        ];
        if !info.players.is_empty() {
            lines.push(Line::from(format!("Online  : {}", info.players.join(", "))));
        }
        let p = Paragraph::new(lines).style(Style::default().fg(Color::White));
        f.render_widget(p, info_area);
    } else {
        let p = Paragraph::new("Pinging server...")
            .style(Style::default().fg(Color::Gray));
        f.render_widget(p, info_area);
    }

    // Log panel
    let log_block = Block::default()
        .title(" Log ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let log_area = log_block.inner(chunks[1]);
    f.render_widget(log_block, chunks[1]);

    let log_items: Vec<ListItem> = app
        .logs
        .iter()
        .rev()
        .take(log_area.height as usize)
        .map(|l| ListItem::new(l.as_str()))
        .collect();
    let log_list = List::new(log_items).style(Style::default().fg(Color::White));
    f.render_widget(log_list, log_area);
}

fn render_flood_tab(f: &mut Frame, area: Rect, _app: &App) {
    let block = Block::default()
        .title(" HTTP Flood ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled(
            "Start a flood from the CLI:",
            Style::default().fg(Color::Gray),
        )),
        Line::from(Span::styled(
            "  cargo run -- http <url> -c 50",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Monitor live stats here during a flood.",
            Style::default().fg(Color::Gray),
        )),
    ];
    let p = Paragraph::new(lines);
    f.render_widget(p, inner);
}

fn render_journal_tab(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Player Journal ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Load journal from disk
    let journal = motd::PlayerJournal::load();
    let key = format!("{}:{}", app.host, app.port);
    if let Some(entries) = journal.players.get(&key) {
        let lines: Vec<Line> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let tag = if e.role == "未知" { String::new() } else { format!(" [{}]", e.role) };
                Line::from(format!("  {}. {}{}", i + 1, e.name, tag))
            })
            .collect();
        let p = Paragraph::new(lines).style(Style::default().fg(Color::White));
        f.render_widget(p, inner);
    } else {
        let p = Paragraph::new("No players recorded yet. Start monitoring with `monitor` CLI command.")
            .style(Style::default().fg(Color::Gray));
        f.render_widget(p, inner);
    }
}

fn render_about_tab(f: &mut Frame, area: Rect, _app: &App) {
    let block = Block::default()
        .title(" About ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled("Creeper", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from(Span::raw("")),
        Line::from(Span::styled("Minecraft stress-test bot + HTTP flood tool", Style::default().fg(Color::White))),
        Line::from(Span::raw("")),
        Line::from(Span::styled("CLI commands:", Style::default().fg(Color::Yellow))),
        Line::from(Span::raw("  info <host>  —  MOTD query")),
        Line::from(Span::raw("  monitor <host> — periodic player collection")),
        Line::from(Span::raw("  http <url>  —  fire-and-forget flood")),
        Line::from(Span::raw("  check <domain> — CDN detection")),
        Line::from(Span::raw("")),
        Line::from(Span::styled("TUI controls:", Style::default().fg(Color::Yellow))),
        Line::from(Span::raw("  Tab/← →  switch tabs")),
        Line::from(Span::raw("  q / Esc   quit")),
    ];
    let p = Paragraph::new(lines).style(Style::default().fg(Color::White));
    f.render_widget(p, inner);
}

/// Fetch node data from the Hub API: combined journal stats + host probes.
async fn fetch_node_data(hub_url: &str) -> String {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build().ok();

    let client = match client {
        Some(c) => c,
        None => return "Failed to create HTTP client".into(),
    };

    let base = hub_url.trim_end_matches('/');

    // Fetch journal stats, host report, and status in parallel
    let journal_fut = client.get(format!("{base}/journal")).send();
    let host_fut = client.get(format!("{base}/host-report")).send();
    let status_fut = client.get(format!("{base}/status")).send();

    let (journal_resp, host_resp, status_resp) = tokio::join!(journal_fut, host_fut, status_fut);

    let mut parts: Vec<String> = Vec::new();

    // Status
    match status_resp {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                parts.push("── Hub Status ──".into());
                if let Some(players) = body.get("total_players").and_then(|v| v.as_u64()) {
                    parts.push(format!("  Total players recorded: {players}"));
                }
                if let Some(servers) = body.get("total_servers").and_then(|v| v.as_u64()) {
                    parts.push(format!("  Total servers discovered: {servers}"));
                }
                parts.push("".into());
            }
        }
        _ => {}
    }

    // Journal
    match journal_resp {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                let players = body.get("players").and_then(|v| v.as_object());
                if let Some(p) = players {
                    parts.push(format!("── Journal: {} servers ──", p.len()));
                    // Count total unique players
                    let total_players: usize = p.values()
                        .filter_map(|v| v.as_array())
                        .map(|a| a.len())
                        .sum();
                    parts.push(format!("  Total entries: {total_players}"));

                    // Breakdown by player count
                    let zero = p.values().filter(|v| v.as_array().map_or(true, |a| a.is_empty())).count();
                    let active = p.len() - zero;
                    parts.push(format!("  Active servers (with players): {active}"));
                    parts.push(format!("  Empty servers: {zero}"));

                    // Top 5 servers by player count
                    let mut servers: Vec<(&String, usize)> = p.iter()
                        .map(|(k, v)| (k, v.as_array().map_or(0, |a| a.len())))
                        .collect();
                    servers.sort_by(|a, b| b.1.cmp(&a.1));
                    parts.push("".into());
                    parts.push("  Top servers:".into());
                    for (srv, count) in servers.iter().take(5) {
                        parts.push(format!("    {srv}: {count} players"));
                    }
                }
            }
        }
        Ok(resp) => {
            parts.push(format!("  Journal API returned: {}", resp.status()));
        }
        Err(e) => {
            parts.push(format!("  Journal fetch failed: {e}"));
        }
    }

    // Host probes
    parts.push("".into());
    match host_resp {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(arr) = body.as_array() {
                    parts.push(format!("── Host Probes: {} nodes ──", arr.len()));
                    for probe in arr.iter().take(5) {
                        let hostname = probe.get("hostname").and_then(|v| v.as_str()).unwrap_or("?");
                        let cpu = probe.get("cpu_pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let ram_used = probe.get("ram_used_bytes").and_then(|v| v.as_u64()).unwrap_or(0);
                        let ram_total = probe.get("ram_total_bytes").and_then(|v| v.as_u64()).unwrap_or(1);
                        let uptime = probe.get("uptime_secs").and_then(|v| v.as_u64()).unwrap_or(0);
                        let ram_pct = ram_used as f64 / ram_total as f64 * 100.0;
                        let uptime_h = uptime / 3600;
                        parts.push(format!("    {hostname}: CPU {cpu:.1}%  RAM {ram_pct:.0}%  up {uptime_h}h"));
                    }
                    if arr.len() > 5 {
                        parts.push(format!("    ... and {} more", arr.len() - 5));
                    }
                }
            }
        }
        Ok(resp) => {
            parts.push(format!("  Host API returned: {}", resp.status()));
        }
        Err(e) => {
            parts.push(format!("  Host fetch failed: {e}"));
        }
    }

    parts.join("\n")
}

/// Render the Nodes tab — shows hub, journal, and host probe data.
fn render_nodes_tab(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(format!(" Nodes — Hub: {} ", app.hub_url))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.node_data.is_empty() {
        let p = Paragraph::new("Connecting to hub...")
            .style(Style::default().fg(Color::Gray));
        f.render_widget(p, inner);
    } else {
        let lines: Vec<Line> = app.node_data.lines()
            .map(|l| {
                let style = if l.starts_with("──") {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else if l.starts_with("  Top") || l.starts_with("  Active") || l.starts_with("  Empty") {
                    Style::default().fg(Color::Yellow)
                } else if l.contains("error") || l.contains("failed") {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(l, style))
            })
            .collect();
        let p = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::White));
        f.render_widget(p, inner);
    }
}

fn render_footer(f: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(Span::styled(
        " Tab/← →: switch  Ctrl+C: quit ",
        Style::default().fg(Color::DarkGray),
    )))
    .style(Style::default().bg(Color::Reset));
    f.render_widget(footer, area);
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run the TUI dashboard.
pub async fn run_tui(hub_url: Option<&str>) -> Result<()> {
    let hub = hub_url.unwrap_or("http://127.0.0.1:9090").to_string();
    let mut app = App::new(&hub);

    // Set up terminal
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    let res = loop {
        terminal.draw(|f| ui(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.should_quit = true;
                        }
                        KeyCode::Tab | KeyCode::Right => {
                            let idx = TABS.iter().position(|t| t == &app.tab).unwrap_or(0);
                            app.tab = TABS[(idx + 1) % TABS.len()];
                        }
                        KeyCode::Left => {
                            let idx = TABS.iter().position(|t| t == &app.tab).unwrap_or(0);
                            app.tab = TABS[(idx + TABS.len() - 1) % TABS.len()];
                        }
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.tick();
            last_tick = Instant::now();
        }

        if app.should_quit {
            break Ok(());
        }
    };

    // Restore terminal
    terminal::disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        cursor::Show
    )?;

    res
}
