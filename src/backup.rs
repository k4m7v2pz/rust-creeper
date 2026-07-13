//! # Backup — 数据打包备份
//!
//! 支持本地备份（打包数据目录）和远程备份（从 Hub API 拉数据再打包）。
//! 输出时间戳命名的 .zip 文件。

use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use zip::write::FileOptions;
use zip::ZipWriter;

/// 获取数据目录路径：优先 CREEPER_DATA_DIR，fallback XDG
fn data_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CREEPER_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    directories::ProjectDirs::from("", "", "creeper")
        .map(|d| d.data_dir().to_path_buf())
}

/// 生成备份文件名：`creeper-data-YYYYMMDD-HHMMSS-backup.zip`
fn zip_filename() -> String {
    let now = Local::now();
    format!("creeper-data-{}-backup.zip", now.format("%Y%m%d-%H%M%S"))
}

/// 备份文件名 glob 模式，用于扫描旧备份
const BACKUP_GLOB: &str = "creeper-data-*-backup.zip";

/// 默认备份输出目录：数据目录下的 `backups/`
fn default_output_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("backups"))
}

/// 压缩并写入一个文件条目
fn zip_add_file(
    zip: &mut ZipWriter<Cursor<Vec<u8>>>,
    archive_name: &str,
    content: &[u8],
) -> Result<(), anyhow::Error> {
    let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    zip.start_file(archive_name, options)?;
    zip.write_all(content)?;
    Ok(())
}

/// 压缩并写入一个 JSON 值
fn zip_add_json<T: serde::Serialize>(
    zip: &mut ZipWriter<Cursor<Vec<u8>>>,
    archive_name: &str,
    value: &T,
) -> Result<(), anyhow::Error> {
    let json = serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| "{}".into());
    zip_add_file(zip, archive_name, json.as_bytes())
}

/// 本地备份：打包数据目录中的文件
fn local_backup_zip(output_dir: &Path) -> Result<PathBuf, anyhow::Error> {
    let data = data_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine data directory"))?;

    fs::create_dir_all(output_dir)?;
    let zip_path = output_dir.join(zip_filename());
    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);

    // 需要打包的文件列表：(归档内路径, 源文件路径, 是否必须)
    let files: Vec<(&str, PathBuf, bool)> = vec![
        ("player-journal.json", data.join("player-journal.json"), true),
        ("task-queue.json",     data.join("task-queue.json"),     false),
        ("mc-targets.json",     data.join("mc-targets.json"),     false),
        ("config.json",         data.join("config.json"),         false),
    ];

    for (archive_name, source_path, required) in &files {
        match fs::read(source_path) {
            Ok(content) => zip_add_file(&mut zip, archive_name, &content)?,
            Err(_) if *required => {
                anyhow::bail!("Required file not found: {}", source_path.display());
            }
            Err(_) => {
                log::debug!("[backup] Skipping (not found): {}", source_path.display());
            }
        }
    }

    // info.txt — 备份元信息
    let info = format!(
        "Creeper Backup\n\
         Generated: {}\n\
         Type: local\n\
         Data dir: {}\n\
         Files: player-journal.json, task-queue.json, mc-targets.json, config.json\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        data.display(),
    );
    zip_add_file(&mut zip, "info.txt", info.as_bytes())?;

    let buf = zip.finish()?;
    fs::write(&zip_path, buf.into_inner())?;

    Ok(zip_path)
}

/// 远程备份：从 Hub API 拉数据再打包
async fn remote_backup_zip(hub_url: &str, output_dir: &Path) -> Result<PathBuf, anyhow::Error> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let hub_base = hub_url.trim_end_matches('/');

    fs::create_dir_all(output_dir)?;
    let zip_path = output_dir.join(zip_filename());
    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);

    // 拉取 /journal
    match client.get(format!("{hub_base}/journal")).send().await {
        Ok(resp) if resp.status().is_success() => {
            let text = resp.text().await.unwrap_or_default();
            zip_add_file(&mut zip, "player-journal.json", text.as_bytes())?;
            log::info!("[backup] Pulled journal from {hub_base}");
        }
        Ok(resp) => log::warn!("[backup] /journal returned {}", resp.status()),
        Err(e) => log::warn!("[backup] Failed to pull /journal: {e}"),
    }

    // 拉取 /status
    match client.get(format!("{hub_base}/status")).send().await {
        Ok(resp) if resp.status().is_success() => {
            let text = resp.text().await.unwrap_or_default();
            zip_add_file(&mut zip, "hub-status.json", text.as_bytes())?;
        }
        _ => {}
    }

    // 拉取 /tasks
    match client.get(format!("{hub_base}/tasks")).send().await {
        Ok(resp) if resp.status().is_success() => {
            let text = resp.text().await.unwrap_or_default();
            zip_add_file(&mut zip, "hub-tasks.json", text.as_bytes())?;
        }
        _ => {}
    }

    // 拉取 /host-report
    match client.get(format!("{hub_base}/host-report")).send().await {
        Ok(resp) if resp.status().is_success() => {
            let text = resp.text().await.unwrap_or_default();
            zip_add_file(&mut zip, "hub-host-reports.json", text.as_bytes())?;
        }
        _ => {}
    }

    // info.txt — 备份元信息
    let info = format!(
        "Creeper Backup\n\
         Generated: {}\n\
         Type: remote\n\
         Hub URL: {}\n\
         Endpoints: /journal, /status, /tasks, /host-report\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        hub_base,
    );
    zip_add_file(&mut zip, "info.txt", info.as_bytes())?;

    let buf = zip.finish()?;
    fs::write(&zip_path, buf.into_inner())?;

    Ok(zip_path)
}

// ── 旧备份清理：最多保留 3 个 ──────────────────

/// 扫描备份目录，删除超出 `keep` 个的最旧备份文件
fn prune_old_backups(dir: &Path, keep: usize) {
    let mut entries: Vec<_> = match fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };
    // 只匹配 `creeper-data-*-backup.zip`
    entries.retain(|e| {
        e.file_name().to_string_lossy().starts_with("creeper-data-")
            && e.file_name().to_string_lossy().ends_with("-backup.zip")
    });
    // 按文件名（含时间戳）排序，最旧的在前面
    entries.sort_by_key(|e| e.file_name());

    if entries.len() <= keep {
        return;
    }

    for old in entries.iter().take(entries.len() - keep) {
        if let Ok(meta) = old.metadata() {
            if meta.is_file() {
                let _ = fs::remove_file(old.path());
                log::info!("[backup] Pruned old backup: {}", old.path().display());
            }
        }
    }
}

// ── 公开入口 ─────────────────────────────────────

/// 执行备份。`hub_url` 为 Some 时远程拉取，为 None 时本地打包。
pub async fn run_backup(
    hub_url: Option<&str>,
    output: Option<&str>,
) -> anyhow::Result<()> {
    let out_dir = match output {
        Some(dir) => PathBuf::from(dir),
        None => default_output_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine default backup directory.\n  Use --output <dir> or set CREEPER_DATA_DIR."))?,
    };

    let zip_path = if let Some(url) = hub_url {
        log::info!("[backup] Remote mode: pulling from {url}");
        println!("🌐 Pulling data from {url} …");
        remote_backup_zip(url, &out_dir).await?
    } else {
        log::info!("[backup] Local mode");
        println!("📦 Packing local data …");
        local_backup_zip(&out_dir)?
    };

    let size_kb = fs::metadata(&zip_path)?.len() as f64 / 1024.0;
    println!("✅ Backup saved: {}", zip_path.display());
    println!("   Size: {:.1} KB", size_kb);

    // 清理旧备份：最多保留 3 个
    prune_old_backups(&out_dir, 3);

    Ok(())
}
