use crate::protocol::GameVersion;

/// Bot attack options — analogue of the Java `Options` class.
#[derive(Debug, Clone)]
pub struct Options {
    pub hostname: String,
    pub port: u16,
    pub amount: usize,
    pub join_delay_ms: u64,
    pub bot_name_format: String,
    pub game_version: GameVersion,
    pub auto_register: bool,
    pub max_attempts: usize,
}
