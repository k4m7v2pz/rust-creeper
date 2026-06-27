/// Initialize logger with custom format (time + level + target + message).
pub fn init() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            use std::io::Write;
            let now: chrono::NaiveDateTime = chrono::Local::now().naive_local();
            writeln!(buf, "{} [{}] [{}] {}", now.format("%I:%M %p"), record.level(), record.target(), record.args())
        })
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_chrono_now_format() {
        let now: chrono::NaiveDateTime = chrono::Local::now().naive_local();
        let s = now.format("%I:%M %p").to_string();
        assert!(s.len() >= 7, "bad time format: {s}");
    }
}
