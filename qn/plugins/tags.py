"""print all unique tags in notes"""

import pathlib
import re


class Plugin:
    def __init__(self, ctx: dict):
        self.ctx = ctx

    def run(self) -> None:
        notes_dir = pathlib.Path(self.ctx['notes_dir'])
        markdown_files = list(notes_dir.rglob('*.md'))
        all_tags = set()
        for file_path in markdown_files:
            try:
                content = file_path.read_text(encoding='utf-8', errors='ignore')
                all_tags.update(re.findall(r'#(\w+)', content))
            except OSError:
                continue
        if all_tags:
            print('unique tags:')
            print('\n'.join(sorted(all_tags, key=str.lower)))
        else:
            print('no tags found.')


def register(parser) -> None:
    pass


def run(ctx: dict) -> None:
    Plugin(ctx).run()


if __name__ == '__main__':
    import json
    import sys

    run(json.load(sys.stdin))
