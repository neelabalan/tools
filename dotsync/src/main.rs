use clap::Parser;
use clap::Subcommand;
use env_logger::Env;
use log::debug;
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

/// Philosophy: Keep Things Simple and Clear
///
/// This tool is designed to be predictable and reliable. It keeps one source of truth:
/// the state file that gets created during initialization. No surprises, no silent updates.
/// The idea is to eliminate confusion about what's installed where and what will happen next.
///
/// Rather than trying to be flexible and supporting every possible workflow, this tool
/// keeps things straightforward: initialize once, set your profile, do your work, then
/// destroy and rebuild if you need to change things. This might sound rigid, but it actually
/// prevents the messy situations that happen when you have half-applied configurations
/// or conflicting setups fighting each other.
///
/// It's built with the everyday scenario in mind: you're setting up a new machine and
/// you want to get your dotfiles in place quickly without wondering
/// "wait, is this going to break something?" The backup happens before we touch anything,
/// so you've always got a snapshot of your original state to fall back on.
///
/// Bottom line: this is straightforward provisioning, not a package manager. Do one thing,
/// do it well, know what's going to happen before you run it.

#[derive(Serialize, Deserialize, Debug)]
struct SymlinkHistory {
    created_at: String,
    backup: String,
    files: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct State {
    url: String,
    branch: String,
    path: String,
    backup_path: String,
    profiles: HashMap<String, Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    active_profile: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    symlink_history: Option<Vec<SymlinkHistory>>,
}

impl State {
    const STATE_FILE_PATH: &'static str = "~/.dotsync.state.json";
    const READONLY_PERMISSIONS: u32 = 0o444;
    const WRITABLE_PERMISSIONS: u32 = 0o644;

    fn new() -> Result<State, Box<dyn std::error::Error>> {
        State::read_state_file()
    }

    fn read_state_file() -> Result<State, Box<dyn std::error::Error>> {
        let path = expand_home(Self::STATE_FILE_PATH);
        let content = fs::read_to_string(path)?;
        let state = serde_json::from_str(&content)?;
        debug!("state file read successfully");
        Ok(state)
    }

    fn set_active_profile(mut self, profile: &String) -> Self {
        self.active_profile = Some(profile.clone());
        self
    }

    fn write_state_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = expand_home(Self::STATE_FILE_PATH);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // make file writable before writing (in case it already exists and is readonly)
        if path.exists() {
            let writable = fs::Permissions::from_mode(Self::WRITABLE_PERMISSIONS);
            fs::set_permissions(&path, writable)?;
            debug!("state file temporarily made writable");
        }

        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json)?;
        debug!("state file written");

        // make file readonly after writing
        let readonly = fs::Permissions::from_mode(Self::READONLY_PERMISSIONS);
        fs::set_permissions(&path, readonly)?;
        info!("state file written and secured as readonly");
        Ok(())
    }
}

#[derive(Parser)]
#[command(
    version,
    about = "a dotfile synchronization tool",
    long_about = "tool to manage and synchronize dotfiles across systems. \
                  Supports multiple profiles, git repositories, and backup/restore functionality."
)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// initialize dotsync by cloning the dotfiles repository
    ///
    /// reads the configuration file, clones the git repository to the specified path,
    /// and creates a state file at ~/.dotsync.state.json (marked as readonly).
    /// this command must be run before any other commands.
    ///
    /// example: dotsync init --config=./config.json
    Init {
        #[arg(short, long, value_name = "FILE")]
        config: Option<std::path::PathBuf>,
    },

    /// set up dotfiles by creating symlinks and backing up existing files
    ///
    /// reads the state file and creates symlinks for all files in the specified profile.
    /// before creating symlinks, existing files are backed up to a timestamped zip file.
    /// if the specified profile is not found, falls back to 'default' profile.
    ///
    /// example: dotsync setup --profile=default
    Setup {
        #[arg(short, long)]
        profile: String,

        #[arg(long)]
        dry_run: bool,
    },

    Status {},
    Refresh {},
    // Validate?
    /// general rust doubt.
    /// how does it interpret the capitlized word as a proper command?
    Backup {},

    Destroy {},
}

fn expand_home(path: &str) -> PathBuf {
    if path.starts_with("~") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let rest = &path[1..];
        PathBuf::from(format!("{}{}", home, rest))
    } else {
        PathBuf::from(path)
    }
}

fn git_is_installed() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn clone_repository(url: &str, branch: &str, path: &PathBuf) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directories: {}", e))?;
    }

    let output = Command::new("git")
        .arg("clone")
        .arg("--branch")
        .arg(branch)
        .arg(url)
        .arg(path)
        .output()
        .map_err(|e| format!("failed to execute git: {}", e))?;

    if output.status.success() {
        info!("successfully cloned repository to {:?}", path);
        Ok(())
    } else {
        Err(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn init(config_path: Option<std::path::PathBuf>) {
    let config_path = match config_path {
        Some(path) => path,
        None => {
            eprintln!("config file path is required");
            return;
        }
    };

    let config_content = match std::fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("failed to read config file: {}", e);
            return;
        }
    };

    let config: State = match serde_json::from_str(&config_content) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("failed to parse config file: {}", e);
            return;
        }
    };

    if !git_is_installed() {
        eprintln!("git is not installed. please install git to proceed.");
        return;
    }

    let repo_path = expand_home(&config.path);
    info!("cloning repository from {} to {:?}", config.url, repo_path);

    if let Err(e) = clone_repository(&config.url, &config.branch, &repo_path) {
        eprintln!("{}", e);
    }

    _ = config.write_state_file();
}

fn create_symlinks(files: &Vec<String>, source_dir: &String) -> Result<(), String> {
    let mut source_path = PathBuf::from(source_dir);
    if source_dir.ends_with('/') {
        source_path = PathBuf::from(source_dir.trim_end_matches('/'))
    }

    for file in files {
        let target = PathBuf::from(file);
        let source = source_path.join(&target);

        if let Some(parent) = target.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create directory {:?}: {}", parent, e))?;
            }
        }

        std::os::unix::fs::symlink(&source, &target).map_err(|e| {
            format!(
                "failed to create symlink {:?} -> {:?}: {}",
                source, target, e
            )
        })?;

        info!("created symlink: {:?} -> {:?}", target, source);
    }
    Ok(())
}

fn create_backup(files: &Vec<String>, target_dir: &str) -> Result<(), String> {
    let backup_path = expand_home(target_dir);

    fs::create_dir_all(&backup_path)
        .map_err(|e| format!("failed to create backup directory {:?}: {}", backup_path, e))?;

    let zip_path = backup_path.join(format!(
        "backup_{}.zip",
        chrono::Local::now().format("%Y%m%d%H%M%S")
    ));

    let mut zip = zip::ZipWriter::new(
        fs::File::create(&zip_path)
            .map_err(|e| format!("failed to create zip file {:?}: {}", zip_path, e))?,
    );
    let options = zip::write::FileOptions::<()>::default();

    for file in files {
        let source = PathBuf::from(file);

        if source.exists() {
            let content = fs::read(&source)
                .map_err(|e| format!("failed to read file {:?}: {}", source, e))?;

            zip.start_file(file, options)
                .map_err(|e| format!("failed to add file to zip: {}", e))?;
            zip.write_all(&content)
                .map_err(|e| format!("failed to write to zip: {}", e))?;
            info!("backed up to zip: {}", file);

            fs::remove_file(file)
                .map_err(|e| format!("failed to remove file from {:?}: {}", file, e))?;
        } else {
            debug!("skipping backup for non-existent file: {:?}", source);
        }
    }
    zip.finish()
        .map_err(|e| format!("failed to finalize zip: {}", e))?;
    Ok(())
}

fn setup(profile: String) {
    let state = match State::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to read state: {}", e);
            return;
        }
    };

    let state = state.set_active_profile(&profile);
    info!("profile set to {}", profile);

    let profile_files = state.profiles.get(&profile).or_else(|| {
        info!("profile {} not found! trying 'default' profile", profile);
        state.profiles.get("default")
    });

    match profile_files {
        Some(files) => {
            if let Err(e) = create_backup(files, &state.backup_path) {
                eprintln!("{}", e);
            }
            if let Err(e) = create_symlinks(files, &state.path) {
                eprintln!("{}", e);
            }
        }
        None => eprintln!("no 'default' profile found!"),
    }
}

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::init_from_env(env);
    let cli = Args::parse();

    match cli.command {
        Commands::Init { config } => init(config),
        Commands::Setup { profile, dry_run } => {
            setup(profile);
            info!("setup command");
        }
        Commands::Status {} => {}
        Commands::Refresh {} => {}
        Commands::Backup {} => {
            info!("Backup command");
            debug!("Backup path: {:?}", "somepath");
        }
        Commands::Destroy {} => {
            info!("Destroy command");
            debug!("Force: {}", "done");
        }
    }
}
