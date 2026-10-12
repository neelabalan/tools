"""show note statistics (files, words, tags, size, dates)"""

import datetime
import pathlib
import re


class Plugin:
    def __init__(self, ctx: dict):
        self.ctx = ctx

    def run(self) -> None:
        notes_dir = pathlib.Path(self.ctx['notes_dir'])

        if not notes_dir.exists():
            print('notes directory does not exist')
            return

        markdown_files = list(notes_dir.rglob('*.md'))

        if not markdown_files:
            print('no markdown files found')
            return

        total_words = 0
        all_tags: set[str] = set()
        total_size = 0
        creation_dates: list[datetime.datetime] = []

        for file_path in markdown_files:
            total_words += self._count_words_in_file(file_path)
            all_tags.update(self._extract_tags_from_file(file_path))
            try:
                total_size += file_path.stat().st_size
            except OSError:
                pass

            creation_dates.append(self._get_file_creation_date(file_path))

        creation_dates.sort()
        earliest = creation_dates[0] if creation_dates else datetime.datetime.min
        latest = creation_dates[-1] if creation_dates else datetime.datetime.min
        size_mb = total_size / (1024 * 1024)

        print('notes statistics:')
        print(f'  total files: {len(markdown_files)}')
        print(f'  total words: {total_words:,}')
        print(f'  unique tags: {len(all_tags)}')
        print(f'  total size: {size_mb:.2f} MB')

        if earliest != datetime.datetime.min:
            print(f'  Earliest note: {earliest.strftime("%Y-%m-%d")}')
            print(f'  Latest note: {latest.strftime("%Y-%m-%d")}')
        if all_tags:
            sorted_tags = sorted(all_tags)
            if len(sorted_tags) <= 10:
                print(f'  Tags: {", ".join(sorted_tags)}')
            else:
                print(f'  Top tags: {", ".join(sorted_tags[:10])}...')

    def _count_words_in_file(self, file_path: pathlib.Path) -> int:
        try:
            content = file_path.read_text(encoding='utf-8', errors='ignore')
            words = re.findall(r'\b\w+\b', content)
            return len(words)
        except OSError:
            return 0

    def _extract_tags_from_file(self, file_path: pathlib.Path) -> set[str]:
        try:
            content = file_path.read_text(encoding='utf-8', errors='ignore')
            return set(re.findall(r'#(\w+)', content))
        except OSError:
            return set()

    def _get_file_creation_date(self, file_path: pathlib.Path) -> datetime.datetime:
        try:
            stat = file_path.stat()
            # st_birthtime is actual creation time on macOS; fall back to st_mtime on Linux
            ctime = getattr(stat, 'st_birthtime', None) or stat.st_mtime
            return datetime.datetime.fromtimestamp(ctime)
        except OSError:
            return datetime.datetime.min


def register(parser) -> None:
    pass


def run(ctx: dict) -> None:
    Plugin(ctx).run()


if __name__ == '__main__':
    import json
    import sys

    run(json.load(sys.stdin))
