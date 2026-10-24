use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub html_url: String,
    pub body: Option<String>,
    pub published_at: Option<String>,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AvailableUpdate {
    pub current_version: String,
    pub latest_version: String,
    pub release_notes: String,
    pub release_url: String,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateStatus {
    Idle,
    Checking,
    Available(Box<AvailableUpdate>),
    Downloading { progress_pct: f32, message: String },
    DownloadedReadyToRestart { new_exe_path: PathBuf },
    UpToDate,
    Failed(String),
}

pub struct UpdateManager {
    status: Arc<Mutex<UpdateStatus>>,
    action_tx: Sender<UpdateAction>,
}

enum UpdateAction {
    CheckNow,
    StartDownload(AvailableUpdate),
    RestartWithNewExe(PathBuf),
}

impl UpdateManager {
    pub fn new() -> Self {
        let status = Arc::new(Mutex::new(UpdateStatus::Idle));
        let (action_tx, action_rx) = channel::<UpdateAction>();

        let status_clone = Arc::clone(&status);
        thread::Builder::new()
            .name("updater_worker".to_string())
            .spawn(move || {
                run_updater_loop(status_clone, action_rx);
            })
            .expect("Failed to spawn updater thread");

        let manager = Self { status, action_tx };
        // Trigger initial check on app startup
        manager.check_for_updates();
        manager
    }

    pub fn status(&self) -> UpdateStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or(UpdateStatus::Idle)
    }

    pub fn check_for_updates(&self) {
        let _ = self.action_tx.send(UpdateAction::CheckNow);
    }

    pub fn start_download(&self, update: AvailableUpdate) {
        let _ = self.action_tx.send(UpdateAction::StartDownload(update));
    }

    pub fn restart_and_apply(&self, new_exe_path: PathBuf) {
        let _ = self.action_tx.send(UpdateAction::RestartWithNewExe(new_exe_path));
    }
}

fn run_updater_loop(status: Arc<Mutex<UpdateStatus>>, action_rx: Receiver<UpdateAction>) {
    while let Ok(action) = action_rx.recv() {
        match action {
            UpdateAction::CheckNow => {
                set_status(&status, UpdateStatus::Checking);
                match fetch_latest_release() {
                    Ok(release) => {
                        let cur_ver = env!("CARGO_PKG_VERSION");
                        if is_newer_version(&release.tag_name, cur_ver) {
                            let asset_url = find_platform_asset(&release.assets);
                            let update = AvailableUpdate {
                                current_version: cur_ver.to_string(),
                                latest_version: release.tag_name.trim_start_matches('v').to_string(),
                                release_notes: release.body.unwrap_or_default(),
                                release_url: release.html_url,
                                download_url: asset_url,
                            };
                            set_status(&status, UpdateStatus::Available(Box::new(update)));
                        } else {
                            set_status(&status, UpdateStatus::UpToDate);
                        }
                    }
                    Err(err) => {
                        // Fail silently or record debug message without annoying user
                        set_status(&status, UpdateStatus::Failed(err));
                    }
                }
            }
            UpdateAction::StartDownload(update) => {
                let Some(download_url) = update.download_url else {
                    set_status(
                        &status,
                        UpdateStatus::Failed("No compatible binary found for your OS.".to_string()),
                    );
                    continue;
                };

                set_status(
                    &status,
                    UpdateStatus::Downloading {
                        progress_pct: 0.1,
                        message: "Downloading update...".to_string(),
                    },
                );

                match download_and_prepare_update(&download_url, &status) {
                    Ok(new_exe) => {
                        set_status(
                            &status,
                            UpdateStatus::DownloadedReadyToRestart {
                                new_exe_path: new_exe,
                            },
                        );
                    }
                    Err(e) => {
                        set_status(&status, UpdateStatus::Failed(e));
                    }
                }
            }
            UpdateAction::RestartWithNewExe(new_exe_path) => {
                if let Err(e) = apply_and_restart(&new_exe_path) {
                    set_status(&status, UpdateStatus::Failed(e));
                }
            }
        }
    }
}

fn set_status(status: &Arc<Mutex<UpdateStatus>>, new_state: UpdateStatus) {
    if let Ok(mut s) = status.lock() {
        *s = new_state;
    }
}

pub fn is_newer_version(remote_tag: &str, current_ver: &str) -> bool {
    let parse_nums = |s: &str| -> (u32, u32, u32) {
        let clean = s.trim().trim_start_matches('v');
        let parts: Vec<&str> = clean.split('.').collect();
        let major = parts.get(0).and_then(|x| x.parse().ok()).unwrap_or(0);
        let minor = parts.get(1).and_then(|x| x.parse().ok()).unwrap_or(0);
        let patch = parts.get(2).and_then(|x| {
            // handle suffixes like 1-beta or 1-1
            let num_part = x.split(|c: char| !c.is_ascii_digit()).next().unwrap_or("0");
            num_part.parse().ok()
        }).unwrap_or(0);
        (major, minor, patch)
    };

    let remote = parse_nums(remote_tag);
    let current = parse_nums(current_ver);
    remote > current
}

fn fetch_latest_release() -> Result<GitHubRelease, String> {
    let url = "https://api.github.com/repos/LycosLane/freedatadiet/releases/latest";

    // Attempt download using curl (standard across Windows 10/11, macOS, and Linux)
    let output = Command::new("curl")
        .args([
            "-sL",
            "-H", "User-Agent: FreeDataDiet-App",
            "-H", "Accept: application/vnd.github.v3+json",
            url,
        ])
        .output()
        .map_err(|e| format!("curl execution error: {e}"))?;

    if !output.status.success() {
        return Err(format!("curl failed with exit code: {:?}", output.status.code()));
    }

    let json_text = String::from_utf8_lossy(&output.stdout);
    if json_text.contains("\"message\":") && json_text.contains("Not Found") {
        return Err("GitHub release not found (or repository is private)".to_string());
    }

    serde_json::from_str::<GitHubRelease>(&json_text)
        .map_err(|e| format!("JSON parsing error: {e}"))
}

fn find_platform_asset(assets: &[ReleaseAsset]) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        for a in assets {
            let lower = a.name.to_lowercase();
            if lower.ends_with(".exe") && (lower.contains("windows") || lower.contains("x64") || lower.contains("freedatadiet")) {
                return Some(a.browser_download_url.clone());
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        for a in assets {
            let lower = a.name.to_lowercase();
            if lower.ends_with(".appimage") {
                return Some(a.browser_download_url.clone());
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        for a in assets {
            let lower = a.name.to_lowercase();
            if lower.ends_with(".dmg") {
                return Some(a.browser_download_url.clone());
            }
        }
    }

    None
}

fn download_and_prepare_update(url: &str, status: &Arc<Mutex<UpdateStatus>>) -> Result<PathBuf, String> {
    let current_exe = env::current_exe().map_err(|e| format!("Cannot locate current executable: {e}"))?;
    let parent_dir = current_exe.parent().unwrap_or_else(|| Path::new("."));

    // Check write permissions in current_exe directory
    let temp_test = parent_dir.join(".fdd_perm_test");
    if fs::write(&temp_test, b"test").is_err() {
        return Err(
            "FreeDataDiet is installed in a system directory (e.g. /usr/bin or Program Files). Please update via your package manager."
                .to_string(),
        );
    }
    let _ = fs::remove_file(&temp_test);

    let new_exe_name = if cfg!(windows) {
        "freedatadiet.exe.new"
    } else {
        "freedatadiet.new"
    };
    let target_file = parent_dir.join(new_exe_name);

    set_status(
        status,
        UpdateStatus::Downloading {
            progress_pct: 0.5,
            message: "Downloading latest binary...".to_string(),
        },
    );

    let output = Command::new("curl")
        .args(["-sL", "-o", target_file.to_str().unwrap(), url])
        .output()
        .map_err(|e| format!("Download failed: {e}"))?;

    if !output.status.success() {
        let _ = fs::remove_file(&target_file);
        return Err("Download process failed to complete.".to_string());
    }

    // On Unix, ensure execute permissions are set
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&target_file) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&target_file, perms);
        }
    }

    Ok(target_file)
}

fn apply_and_restart(new_exe_path: &Path) -> Result<(), String> {
    let current_exe = env::current_exe().map_err(|e| format!("Current exe not found: {e}"))?;
    let backup_exe = if cfg!(windows) {
        current_exe.with_extension("exe.old")
    } else {
        current_exe.with_extension("old")
    };

    // Remove old backup if it exists
    let _ = fs::remove_file(&backup_exe);

    // Rename running exe to .old (Windows fully permits renaming a running binary!)
    fs::rename(&current_exe, &backup_exe)
        .map_err(|e| format!("Failed to move current executable: {e}"))?;

    // Move new exe into active executable place
    if let Err(e) = fs::rename(new_exe_path, &current_exe) {
        // Rollback on failure
        let _ = fs::rename(&backup_exe, &current_exe);
        return Err(format!("Failed to activate new executable: {e}"));
    }

    // Launch the updated application
    Command::new(&current_exe)
        .spawn()
        .map_err(|e| format!("Failed to spawn updated application: {e}"))?;

    // Exit current instance cleanly
    std::process::exit(0);
}
