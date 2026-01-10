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
struct History {
    created_at: String,
    backup: String,
    files: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct State {
    url: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,

    path: String,
    backup_path: String,
    profiles: HashMap<String, Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    active_profile: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    history: Option<Vec<History>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    source_type: Option<SourceType>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
enum SourceType {
    Zip,
    GitHttps,
    GitSsh,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceType::Zip => write!(f, "zip"),
            SourceType::GitHttps => write!(f, "git-https"),
            SourceType::GitSsh => write!(f, "git-ssh"),
        }
    }
}

fn detect_source_type(url: &str) -> SourceType {
    if url.contains("/archive/") || url.contains("/zipball/") || url.contains("/releases/download/")
    {
        return SourceType::Zip;
    }

    if url.starts_with("git@") || url.starts_with("ssh://") {
        return SourceType::GitSsh;
    }

    SourceType::GitHttps
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

    fn set_active_profile(mut self, profile: &str) -> Self {
        self.active_profile = Some(profile.to_owned());
        self
    }

    fn set_source_type(mut self, source_type: SourceType) -> Self {
        self.source_type = Some(source_type);
        self
    }

    fn append_history(mut self, history: History) -> Self {
        self.history.get_or_insert_with(Vec::new).push(history);
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

    fn remove_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        let state_file_path = expand_home(Self::STATE_FILE_PATH);
        if state_file_path.exists() {
            std::fs::remove_file(state_file_path)?;
        } 
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

    /// show the current active profile and synced dotfiles
    ///
    /// displays the active profile and lists all symlinked files.
    /// files marked with '+' are properly symlinked, '-' indicates issues.
    ///
    /// example: dotsync status
    Status {},

    /// refresh dotfiles from the repository
    ///
    /// pulls latest changes from the repository and updates symlinks.
    /// useful for keeping your dotfiles in sync across machines.
    ///
    /// example: dotsync refresh
    Refresh {},

    /// create a backup of current dotfiles
    ///
    /// creates a timestamped zip file of all dotfiles in the active profile.
    /// backup is saved to the configured backup_path.
    ///
    /// example: dotsync backup
    Backup {},

    /// remove all symlinks for the active profile
    ///
    /// removes all symlinks created by dotsync for the active profile.
    /// this does not delete your actual dotfiles, only the symlinks.
    /// the state file is also removed after cleanup.
    ///
    /// example: dotsync destroy
    Destroy {},
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~") {
        PathBuf::from(format!(
            "{}{}",
            std::env::var("HOME").unwrap_or_else(|_| ".".to_string()),
            rest
        ))
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

    let mut cmd = Command::new("git");
    cmd.arg("clone");

    if !branch.is_empty() {
        cmd.arg("--branch").arg(branch);
    }

    cmd.arg(url).arg(path);

    let output = cmd
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

fn download_and_extract_zip(url: &str, path: &PathBuf) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directories: {}", e))?;
    }

    let temp_zip = path.with_extension("zip.tmp");

    let output = Command::new("curl")
        .arg("-L")
        .arg("-o")
        .arg(&temp_zip)
        .arg(url)
        .output()
        .map_err(|e| format!("failed to execute curl: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    info!("downloaded zip file from {}", url);

    let output = Command::new("unzip")
        .arg("-q")
        .arg(&temp_zip)
        .arg("-d")
        .arg(path)
        .output()
        .map_err(|e| format!("failed to execute unzip: {}", e))?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&temp_zip);
        return Err(format!(
            "unzip failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    std::fs::remove_file(&temp_zip)
        .map_err(|e| format!("failed to remove temporary zip file: {}", e))?;

    info!("successfully extracted zip to {:?}", path);
    Ok(())
}

fn init(config_path: Option<std::path::PathBuf>) -> Result<(), String> {
    let config_path = config_path.ok_or("config file path is required")?;

    let config_content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("failed to read config file: {}", e))?;

    let config: State = serde_json::from_str(&config_content)
        .map_err(|e| format!("failed to parse config file: {}", e))?;

    let source_type = detect_source_type(&config.url);
    info!("detected source type: {}", source_type);

    let repo_path = expand_home(&config.path);

    match source_type {
        SourceType::Zip => {
            info!("downloading zip from {}", config.url);
            download_and_extract_zip(&config.url, &repo_path)
        }
        SourceType::GitHttps | SourceType::GitSsh => {
            if !git_is_installed() {
                return Err("git is not installed. please install git to proceed.".to_string());
            }
            let branch = config.branch.as_deref().unwrap_or("");
            info!("cloning repository from {} to {:?}", config.url, repo_path);
            clone_repository(&config.url, branch, &repo_path)
        }
    }?;

    let config = config.set_source_type(source_type);
    config
        .write_state_file()
        .map_err(|e| format!("failed to write state file: {}", e))?;

    Ok(())
}

// TODO: implement rollback logic - if symlink creation fails midway,
// already-created symlinks should be cleaned up to avoid orphaned state
fn create_symlinks(files: &Vec<String>, source_dir: &str) -> Result<String, String> {
    let source_path = expand_home(source_dir.trim_end_matches('/'));

    for file in files {
        let target = PathBuf::from(file);
        let source = source_path.join(&target);

        if let Some(parent) = target.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directory {:?}: {}", parent, e))?;
        }

        std::os::unix::fs::symlink(&source, &target).map_err(|e| {
            format!(
                "failed to create symlink {:?} -> {:?}: {}",
                source, target, e
            )
        })?;

        info!("created symlink: {:?} -> {:?}", target, source);
    }
    Ok(chrono::Local::now()
        .format("%Y-%m-%d--%H-%M-%S")
        .to_string())
}

fn create_backup(files: &Vec<String>, target_dir: &str) -> Result<PathBuf, String> {
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

    info!("backup created at {:?}", backup_path);
    Ok(zip_path)
}

fn setup(profile: String) -> Result<(), String> {
    let mut state = State::new().map_err(|e| format!("failed to read state: {}", e))?;
    state = state.set_active_profile(&profile);
    info!("profile set to {}", profile);

    let profile_files = state
        .profiles
        .get(&profile)
        .or_else(|| {
            info!("profile {} not found! trying 'default' profile", profile);
            state.profiles.get("default")
        })
        .ok_or("no 'default' profile found!")?;

    let backup_dir = create_backup(profile_files, &state.backup_path)?;
    info!("backup completed at {:?}", backup_dir);

    let created_at = create_symlinks(profile_files, &state.path)?;

    let history = History {
        created_at,
        backup: backup_dir.display().to_string(),
        files: profile_files.clone(),
    };
    state = state.append_history(history);
    state
        .write_state_file()
        .map_err(|e| format!("failed to write state file: {}", e))?;

    Ok(())
}

fn status() -> Result<(), String> {
    let state = State::new().map_err(|e| format!("failed to read state: {}", e))?;

    match &state.active_profile {
        Some(profile) => {
            println!("active profile: {}", profile);
            println!();

            if let Some(files) = state.profiles.get(profile) {
                println!("synced dotfiles ({}):", files.len());
                for file in files {
                    let status = if expand_home(file).is_symlink() {
                        "+"
                    } else {
                        "-"
                    };
                    println!("  {} {}", status, file);
                }
            } else {
                println!("no files found for profile '{}'", profile);
            }
        }
        None => {
            println!("no active profile set.");
            println!("run 'dotsync setup --profile=<name>' first.")
        }
    }

    Ok(())
}

fn destroy() -> Result<(), String> {
    let state = State::new().map_err(|e| format!("failed to read state: {}", e))?;
    match &state.active_profile {
        Some(profile) => {
            println!("active profile: {}", profile);
            if let Some(files) = state.profiles.get(profile) {
                for file in files {
                    match std::fs::remove_file(expand_home(file)) {
                        Ok(_) => println!("removed {}", file),
                        Err(e) => eprintln!("couldn't remove symlink for file {}: {:?}", file, e),
                    }
                }
            } else {
                println!("no files found for profile '{}'", profile);
            }
        }
        None => {
            println!("no active profile set.");
        }
    }
    state
        .remove_file()
        .map_err(|e| format!("failed to remove state file: {}", e))?;
    Ok(())
}

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::init_from_env(env);
    let cli = Args::parse();

    let result: Result<(), String> = match cli.command {
        Commands::Init { config } => init(config),
        Commands::Setup { profile, .. } => setup(profile),
        Commands::Status {} => status(),
        Commands::Destroy {} => destroy(),
        Commands::Refresh {} => Ok(()),
        Commands::Backup {} => {
            info!("Backup command");
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
