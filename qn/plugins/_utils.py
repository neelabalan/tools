# shared helpers for qn plugins. files starting with "_" are helper modules, not plugins.
import json
import pathlib
import subprocess


def expand_path(path_str: str) -> pathlib.Path:
    if not path_str or '..' in path_str:
        raise ValueError(f'invalid path: {path_str}')
    return pathlib.Path(path_str).expanduser().resolve()


def open_in_editor(ctx: dict, file_path: pathlib.Path, also_open_directory: bool = False) -> None:
    editor = ctx['config'].get('editor', 'vim')
    if not editor or any(c in editor for c in ';&|`') or '$(' in editor:
        print(f'invalid editor command: {editor}')
        return

    cmd = [editor, ctx['notes_dir'], str(file_path)] if also_open_directory else [editor, str(file_path)]
    try:
        subprocess.run(cmd, check=True)
    except subprocess.CalledProcessError:
        print(f'failed to open {file_path} with {editor}')
    except FileNotFoundError:
        print(f"editor '{editor}' not found")


def save_config(ctx: dict, config: dict) -> None:
    config_path = pathlib.Path(ctx['config_path'])
    config_path.parent.mkdir(parents=True, exist_ok=True)
    config_path.write_text(json.dumps(config, indent=4))
    config_path.chmod(0o600)
