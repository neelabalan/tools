"""command manager - bookmark and fzf-pick shell commands (markdown is source of truth)"""

import dataclasses
import pathlib
import re
import shutil
import subprocess
import sys

from _utils import open_in_editor

DELIMTER = '\x1f'
ENTRY_SEPARATOR = '---'
KEY_LINE_RE = re.compile(r'^(command|tag|description):\s?(.*)$')


def _prompt(prompt: str) -> str:
    # qn passes ctx via stdin, so interactive prompts must read from the terminal
    if sys.stdin.isatty():
        return input(prompt)
    try:
        with open('/dev/tty') as tty:
            print(prompt, end='', flush=True)
            return tty.readline().strip()
    except OSError:
        print(prompt, end='')
        return ''


@dataclasses.dataclass(frozen=True)
class CommandEntry:
    command: str
    tags: tuple[str, ...]
    description: str


class Plugin:
    def __init__(self, ctx: dict):
        self.ctx = ctx

    def run(self) -> None:
        args = self.ctx['args']
        command_file = self._command_file()

        match args.get('cm_command'):
            case 'new':
                self._new(command_file, args)
            case 'add':
                self._add(command_file)
            case 'edit':
                open_in_editor(self.ctx, command_file)
            case 'where':
                print(command_file)
            case 'pick' | None:
                self._pick(command_file)
            case _:
                print('usage: qn cm {new,add,edit,where,pick}')

    def _command_file(self) -> pathlib.Path:
        notes_dir = pathlib.Path(self.ctx['notes_dir'])
        return notes_dir / self.ctx['config'].get('command_file', 'command.md')

    def _parse_block(self, block: str) -> CommandEntry | None:
        lines = block.splitlines()
        line_count = len(lines)
        index = 0

        while index < line_count and not lines[index].strip().startswith('command:'):
            index += 1
        if index >= line_count:
            return None

        command_lines = [lines[index].split(':', 1)[1].strip()]
        index += 1

        # a command may span multiple lines; keep consuming until the next recognized key
        while index < line_count:
            match = KEY_LINE_RE.match(lines[index])
            if match and match.group(1) in ('tag', 'description'):
                break
            command_lines.append(lines[index])
            index += 1
        tags: tuple[str, ...] = ()
        description = ''

        while index < line_count:
            match = KEY_LINE_RE.match(lines[index])
            if match and match.group(1) == 'tag':
                tags = tuple(t.lstrip('#') for t in match.group(2).split())
            elif match and match.group(1) == 'description':
                description = match.group(2).strip()
            index += 1

        command = '\n'.join(command_lines).strip('\n')
        if not command:
            return None

        return CommandEntry(command=command, description=description, tags=tags)

    def _split_blocks(self, text: str) -> list[str]:
        blocks = []
        current: list[str] = []
        for line in text.splitlines():
            if line.strip() == ENTRY_SEPARATOR:
                if current:
                    blocks.append('\n'.join(current))
                current = []
            else:
                current.append(line)
        if current:
            blocks.append('\n'.join(current))
        return blocks

    def parse_commands(self, text: str) -> list[CommandEntry]:
        entries = []
        for block in self._split_blocks(text):
            entry = self._parse_block(block)
            if entry is not None:
                entries.append(entry)
        return entries

    def _new(self, command_file: pathlib.Path, args: dict) -> None:
        command_text = args.get('command_text') or _prompt('command: ').strip()
        if not command_text:
            print('no command given, aborting')
            return
        tags = args.get('tag') or [t for t in _prompt('tag (comma separated): ').strip().split(',') if t.strip()]
        description = args.get('description', '') or _prompt('description: ').strip()
        self._append_entry(command_file, command_text, tags, description)

    def _add(self, command_file: pathlib.Path) -> None:
        if shutil.which('fzf') is None:
            print('fzf is required for "add"')
            return

        history_lines = self._shell_history()
        if not history_lines:
            print('no shell history found')
            return

        result = subprocess.run(['fzf'], input='\n'.join(history_lines), capture_output=True, text=True)
        command_text = result.stdout.strip()
        if not command_text:
            return
        tags = [t.strip() for t in _prompt('tag (comma separated): ').strip().split(',') if t.strip()]
        description = _prompt('description: ').strip()
        self._append_entry(command_file, command_text, tags, description)

    def _pick(self, command_file: pathlib.Path) -> None:
        if shutil.which('fzf') is None:
            print('fzf is required for "pick"')
            return
        if not command_file.exists():
            print(f'no command file at {command_file}')
            return

        entries = self.parse_commands(command_file.read_text(encoding='utf-8', errors='ignore'))
        if not entries:
            print('no commands saved yet')
            return

        fzf_input = []
        command_map = {}
        for entry in entries:
            tags = ' '.join(f'#{t}' for t in entry.tags)
            fzf_input.append(DELIMTER.join((entry.command, tags, entry.description)))
            command_map[entry.command] = (tags, entry.description)

        fzf_command = [
            'fzf',
            '--preview-window=up:20%',
            f'--delimiter={DELIMTER}',
            '--with-nth=1',
            '--preview', "echo -e 'Tag: {2}\\nDescription: {3}'",
        ]
        result = subprocess.run(fzf_command, input='\n'.join(fzf_input), capture_output=True, text=True)
        if result.returncode != 0 or not result.stdout.strip():
            return
        command_text = result.stdout.strip().split(DELIMTER)[0]
        if command_text:
            print(command_text)

    def _shell_history(self) -> list[str]:
        for history_file in (pathlib.Path.home() / '.bash_history',):
            try:
                lines = history_file.read_text(encoding='utf-8', errors='ignore').splitlines()
            except OSError:
                continue
            cleaned = (re.sub(r'^: \d+:\d+;', '', line) for line in lines)  # strip zsh history timestamps
            return list(dict.fromkeys(reversed(list(cleaned))))
        return []

    def _append_entry(self, command_file: pathlib.Path, command_text: str, tags: list[str], description: str) -> None:
        command_file.parent.mkdir(parents=True, exist_ok=True)

        normalized_tags = []
        for tag in tags:
            for part in tag.split(','):
                cleaned = part.strip().lstrip('#')
                if cleaned and cleaned not in normalized_tags:
                    normalized_tags.append(cleaned)
        tags = normalized_tags

        entry_lines = [f'command: {command_text}']
        if tags:
            entry_lines.append('tag: ' + ' '.join(f'#{tag}' for tag in tags))
        if description:
            entry_lines.append(f'description: {description}')

        needs_separator = command_file.exists() and command_file.stat().st_size > 0
        with open(command_file, 'a') as f:
            if needs_separator:
                f.write(ENTRY_SEPARATOR + '\n')
            f.write('\n'.join(entry_lines) + '\n')
        print(f'appended to {command_file}')


def register(parser) -> None:
    subparsers = parser.add_subparsers(dest='cm_command')

    new_parser = subparsers.add_parser(
        'new', help='append a new command entry (opens a prompt unless --command is given)'
    )
    new_parser.add_argument('--command', dest='command_text')
    new_parser.add_argument('--tag', action='append', default=[])
    new_parser.add_argument('--description', default='')

    subparsers.add_parser('add', help='pick a command from shell history and append it')
    subparsers.add_parser('edit', help='open commands.md in the editor')
    subparsers.add_parser('where', help='print the path to commands.md')
    subparsers.add_parser('pick', help='fzf-pick a saved command and print it (pipe into a shell wrapper)')


def run(ctx: dict) -> None:
    Plugin(ctx).run()


if __name__ == '__main__':
    import json
    import sys

    run(json.load(sys.stdin))
