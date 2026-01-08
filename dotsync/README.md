# dotsync

A simple dotfile synchronization tool written in Rust. Manages your configuration files across systems using git repositories and profiles.

### Overview

- `dotsync` helps you maintain dotfiles by:
    - Cloning your dotfiles repository
    - Creating symlinks to your config files
    - Backing up existing files before making changes
    - Supporting multiple profiles for different setups

The tool keeps one source of truth: a readonly state file that tracks your configuration. Initialize once, set a profile, and you're done.


### Quick start

1. Create a configuration file
2. Run `dotsync init --config .dotsync.json`
3. Run `dotsync setup --profile default`

### Configuration file sample

```json
{
    "url": "https://github.com/neelabalan/dotfiles",
    "branch": "master",
    "path": "~/.dotfiles",
    "backup_path": "~/.dotfiles/backup/",
    "profiles": {
        "default": [
            ".bashrc",
            ".bash_profile",
            ".tmux.conf",
            ".haskeline",
            ".ruff.toml",
            ".vimrc",
            ".visidatarc",
            ".ipython",
            ".config/starship.toml",
            ".config/nvim",
            ".config/.ghc",
            ".config/kitty",
            ".config/mpv",
            ".config/ranger"
        ],
        "container": [
            ".bashrc",
            ".bash_profile",
            ".config/starship.toml",
            ".config/nvim",
            ".tmux.conf"
        ]
    }

}
```

### State file sample

> `~/.dotsync.state.json` (read only file)

```json
{
    "url": "https://github.com/neelabalan/dotfiles",
    "branch": "master",
    "path": "~/.dotfiles",
    "backup_path": "~/.dotfiles/backup/",
    "active_profile": "default",
    "history": [
        {
            "created_at": "2024-01-06T10:35:00Z",
            "backup": "~/.dotfiles/backup/dotfiles_backup_2024-01-06.zip",
            "files": [
                ".bashrc"
            ]
        }
    ],
    "profiles": {
        "default": [
            ".bashrc",
            ".bash_profile",
            ".tmux.conf",
            ".haskeline",
            ".ruff.toml",
            ".vimrc",
            ".visidatarc",
            ".ipython",
            ".config/starship.toml",
            ".config/nvim",
            ".config/.ghc",
            ".config/kitty",
            ".config/mpv",
            ".config/ranger"
        ],
        "container": [
            ".bashrc",
            ".bash_profile",
            ".config/starship.toml",
            ".config/nvim",
            ".tmux.conf"
        ]
    }
}
```

### Workflow

Initialize once:

```bash
dotsync init --config /path/to/.dotsync.json
```

Set up a profile:

```bash
dotsync setup --profile default
```

This will:
- Back up existing files to `~/.dotfiles_backup/backup_YYYYMMDDHHMMSS.zip`
- Create symlinks from `~/.dotfiles` to your home directory
- Update the state file with your active profile



### Error handling

All errors are logged with context. Check log output with:

```bash
LOG_LEVEL=debug dotsync setup --profile default
```