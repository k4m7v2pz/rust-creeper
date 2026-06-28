//! Terminal UI (TUI) dashboard for LambdaAttack.
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
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap};
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
    About,
}

const TABS: &[Tab] = &[Tab::Monitor, Tab::Flood, Tab::Journal, Tab::About];

impl Tab {
    fn label(&self) -> &'static str {
        match self {
            Self::Monitor => " Monitor ",
            Self::Flood => " Flood ",
            Self::Journal => " Journal ",
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
}

impl App {
    fn new() -> Self {
        Self {
            tab: Tab::About,
            should_quit: false,
            last_result: None,
            last_ping: Instant::now(),
            ping_interval: 0,
            host: String::new(),
            port: 0,
            logs: vec![],
        }
    }

    fn tick(&mut self) {
        // Ping only when an interval is configured and a server is set
        if self.ping_interval == 0 || self.host.is_empty() {
            return;
        }
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
        format!(" LambdaAttack v{} ", env!("CARGO_PKG_VERSION")),
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
        Line::from(Span::styled("LambdaAttack", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
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
pub async fn run_tui() -> Result<()> {
    let mut app = App::new();

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
