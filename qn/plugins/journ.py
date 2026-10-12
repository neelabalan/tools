"""daily, weekly and monthly notes"""

import datetime
import pathlib

from _utils import open_in_editor


class Plugin:
    def __init__(self, ctx: dict):
        self.ctx = ctx

    def run(self) -> None:
        args = self.ctx['args']
        command = args.get('journ_command') or 'daily'

        if command == 'weekly':
            week_offset = {'previous': -1, 'next': 1}.get(args.get('weekly_command'), 0)
            self._create_weekly_note(week_offset)
        elif command == 'monthly':
            self._create_monthly_note()
        else:
            self._create_daily_note()

    def _create_daily_note(self) -> None:
        notes_dir = pathlib.Path(self.ctx['notes_dir'])
        today = datetime.date.today()
        daily_path = notes_dir / 'daily' / f'{today.strftime("%Y%m%d")}.md'

        daily_path.parent.mkdir(parents=True, exist_ok=True)
        daily_path.touch()

        open_in_editor(self.ctx, daily_path, also_open_directory=True)

    def _create_weekly_note(self, week_offset: int = 0) -> None:
        notes_dir = pathlib.Path(self.ctx['notes_dir'])
        today = datetime.date.today()
        target_date = today + datetime.timedelta(weeks=week_offset)
        week_start = target_date - datetime.timedelta(days=target_date.weekday())
        week_end = week_start + datetime.timedelta(days=6)

        weekly_dir = notes_dir / 'weekly'
        weekly_dir.mkdir(parents=True, exist_ok=True)

        weekly_filename = f'{week_start.strftime("%Y%m%d")}-{week_end.strftime("%Y%m%d")}.md'
        weekly_path = weekly_dir / weekly_filename
        weekly_path.touch()

        open_in_editor(self.ctx, weekly_path, also_open_directory=True)

    def _create_monthly_note(self) -> None:
        notes_dir = pathlib.Path(self.ctx['notes_dir'])
        today = datetime.date.today()
        monthly_path = notes_dir / 'monthly' / f'{today.strftime("%Y%m")}.md'

        monthly_path.parent.mkdir(parents=True, exist_ok=True)
        monthly_path.touch()

        open_in_editor(self.ctx, monthly_path, also_open_directory=True)


def register(parser) -> None:
    subparsers = parser.add_subparsers(dest='journ_command')

    subparsers.add_parser('daily', help='create/open daily notes (default)')

    weekly_parser = subparsers.add_parser('weekly', help='create/open weekly notes')
    weekly_subparsers = weekly_parser.add_subparsers(dest='weekly_command')
    weekly_subparsers.add_parser('current', help='create/open current week note (default)')
    weekly_subparsers.add_parser('previous', help='create/open previous week note')
    weekly_subparsers.add_parser('next', help='create/open next week note')

    subparsers.add_parser('monthly', help='create/open monthly note')


def run(ctx: dict) -> None:
    Plugin(ctx).run()


if __name__ == '__main__':
    import json
    import sys

    run(json.load(sys.stdin))
