use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::options::Options;
use crate::protocol::{ProxyInfo, SessionListener, UniversalProtocol};

const COMMAND_IDENTIFIER: char = '/';

struct BotSessionListener {
    bot_name: String,
    auto_register: bool,
    connected: Arc<AtomicBool>,
    command_tx: Option<tokio::sync::mpsc::Sender<BotCommand>>,
}

impl BotSessionListener {
    fn new(bot_name: String, auto_register: bool, command_tx: Option<tokio::sync::mpsc::Sender<BotCommand>>) -> Self {
        Self { bot_name, auto_register, connected: Arc::new(AtomicBool::new(false)), command_tx }
    }
}

impl SessionListener for BotSessionListener {
    fn on_disconnected(&self, reason: &str) {
        self.connected.store(false, Ordering::SeqCst);
        log::info!("[{}] Disconnected: {}", self.bot_name, reason);
    }
    fn on_join(&self) {
        self.connected.store(true, Ordering::SeqCst);
        log::info!("[{}] Joined", self.bot_name);
        if self.auto_register {
            log::info!("[{}] Auto-register: sending /register and /login", self.bot_name);
            if let Some(ref tx) = self.command_tx {
                let _ = tx.try_send(BotCommand::SendMessage("/register password123".into()));
                let _ = tx.try_send(BotCommand::SendMessage("/login password123".into()));
            }
        }
    }
    fn on_chat_message(&self, msg: &str) { log::info!("[{}] {}", self.bot_name, msg); }
    fn on_position_update(&self, _x: f64, _y: f64, _z: f64, _pitch: f32, _yaw: f32) {}
    fn on_health_update(&self, _health: f32, _food: f32) {}
}

enum BotCommand {
    SendMessage(String),
    Disconnect(String),
}

/// Handle to a running bot task.
pub struct BotHandle {
    _name: String,
    session_tx: tokio::sync::mpsc::Sender<BotCommand>,
    _connected: Arc<AtomicBool>,
}

#[allow(dead_code)]
pub struct Bot;

impl Bot {
    /// Spawn a bot with retry support.
    /// On disconnect/kick, if `max_attempts > 1` the bot will reconnect
    /// with a new proxy (if available).
    pub fn spawn(options: Options, protocol: Box<dyn UniversalProtocol>, proxy: Option<ProxyInfo>) -> BotHandle {
        let bot_name = protocol.profile().name.clone();
        let max_attempts = options.max_attempts;
        let bot_name_clone = bot_name.clone();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<BotCommand>(64);
        let tx_clone = tx.clone(); // clone before moving tx into the spawn
        let connected = Arc::new(AtomicBool::new(false));
        let conn = connected.clone();
        let host = options.hostname.clone();
        let port = options.port;
        let auto_register = options.auto_register;

        tokio::spawn(async move {
            let mut attempt = 0;
            loop {
                attempt += 1;
                if attempt > max_attempts {
                    log::info!("[{}] Max attempts ({}) reached", bot_name_clone, max_attempts);
                    break;
                }
                if attempt > 1 {
                    log::info!("[{}] Attempt #{}/{}", bot_name_clone, attempt, max_attempts);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }

                let listener = Arc::new(BotSessionListener::new(bot_name_clone.clone(), auto_register, Some(tx.clone())));
                let connected2 = listener.connected.clone();
                let mut session = protocol.connect(&host, port, proxy.as_ref(), listener);
                conn.store(true, Ordering::SeqCst);

                // Wait for commands until disconnect
                loop {
                    tokio::select! {
                        cmd = rx.recv() => {
                            match cmd {
                                Some(BotCommand::SendMessage(msg)) => session.send_chat(&msg),
                                Some(BotCommand::Disconnect(reason)) => {
                                    session.disconnect(&reason);
                                    conn.store(false, Ordering::SeqCst);
                                    return; // explicit disconnect — do not retry
                                }
                                None => { conn.store(false, Ordering::SeqCst); return; }
                            }
                        }
                        _ = async {
                            // Poll connection status periodically
                            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        } => {
                            if !connected2.load(Ordering::SeqCst) {
                                // Session disconnected — retry if attempts remain
                                conn.store(false, Ordering::SeqCst);
                                break;
                            }
                        }
                    }
                }
            }
        });

        BotHandle { _name: bot_name, session_tx: tx_clone, _connected: connected }
    }

    pub fn command_identifier() -> char { COMMAND_IDENTIFIER }
}

impl BotHandle {
    pub async fn send_message(&self, msg: String) { let _ = self.session_tx.send(BotCommand::SendMessage(msg)).await; }
    pub async fn disconnect(&mut self, reason: &str) { let _ = self.session_tx.send(BotCommand::Disconnect(reason.into())).await; }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_command_identifier() { assert_eq!(Bot::command_identifier(), '/'); }
}
