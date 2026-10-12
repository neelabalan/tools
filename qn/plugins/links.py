"""manage URLs in notes"""

import concurrent.futures
import datetime
import html.parser
import http.server
import json
import pathlib
import re
import socketserver
import sqlite3
import threading
import typing
import urllib.error
import urllib.parse
import urllib.request
import webbrowser


class TitleExtractor(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.in_title = False
        self.title = ''

    def handle_starttag(self, tag, attrs):
        if tag.lower() == 'title':
            self.in_title = True

    def handle_endtag(self, tag):
        if tag.lower() == 'title':
            self.in_title = False

    def handle_data(self, data):
        if self.in_title:
            self.title += data


class LinkManager:
    def __init__(self, notes_dir: pathlib.Path, max_workers: int = 10):
        self.notes_dir = notes_dir
        self.max_workers = max_workers
        self.db_path = pathlib.Path.home() / '.config/qn/qn.db'
        self._init_db()

    def _init_db(self) -> None:
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.cursor()

            cursor.execute("""
                CREATE TABLE IF NOT EXISTS links (
                    url TEXT PRIMARY KEY,
                    title TEXT,
                    status TEXT,
                    status_code INTEGER,
                    last_checked TIMESTAMP,
                    domain TEXT,
                    first_seen TIMESTAMP,
                    file_path TEXT
                )
            """)

            cursor.execute('CREATE INDEX IF NOT EXISTS idx_domain ON links(domain)')

            cursor.execute("""
                CREATE TABLE IF NOT EXISTS tags (
                    tag_name TEXT PRIMARY KEY,
                    link_count INTEGER DEFAULT 0
                )
            """)

            cursor.execute("""
                CREATE TABLE IF NOT EXISTS link_tags (
                    url TEXT NOT NULL,
                    tag_name TEXT NOT NULL,
                    PRIMARY KEY (url, tag_name),
                    FOREIGN KEY (url) REFERENCES links(url) ON DELETE CASCADE,
                    FOREIGN KEY (tag_name) REFERENCES tags(tag_name) ON DELETE CASCADE
                )
            """)

            cursor.execute('CREATE INDEX IF NOT EXISTS idx_link_tags_url ON link_tags(url)')
            cursor.execute('CREATE INDEX IF NOT EXISTS idx_link_tags_tag ON link_tags(tag_name)')

    def extract_urls_from_file(self, file_path: pathlib.Path) -> list[tuple[str, int]]:
        urls = []
        try:
            content = file_path.read_text(encoding='utf-8', errors='ignore')
            lines = content.split('\n')

            in_code_block = False
            for line_num, line in enumerate(lines, 1):
                if line.strip().startswith('```'):
                    in_code_block = not in_code_block
                    continue

                if in_code_block:
                    continue

                url_pattern = r'https?://[^\s\)\]>]+'
                matches = re.finditer(url_pattern, line)
                for match in matches:
                    url = match.group(0)
                    url = url.rstrip('.,;:!?')
                    urls.append((url, line_num))

        except OSError:
            pass

        return urls

    def extract_tagged_urls_from_file(self, file_path: pathlib.Path) -> dict[str, tuple[list[str], int]]:
        url_tags = {}
        try:
            content = file_path.read_text(encoding='utf-8', errors='ignore')
            lines = content.split('\n')

            line_index = 0
            while line_index < len(lines):
                current_line = lines[line_index].strip()

                if '<!-- qn-links -->' in current_line or '<!--qn-links-->' in current_line:
                    line_index += 1
                    tag_stack: list[tuple[int, str]] = []

                    while line_index < len(lines):
                        current_line = lines[line_index]
                        stripped_line = current_line.strip()

                        if '<!-- /qn-links -->' in stripped_line or '<!--/qn-links-->' in stripped_line:
                            break

                        if not stripped_line or stripped_line.startswith('```'):
                            line_index += 1
                            continue

                        indent_level = len(current_line) - len(current_line.lstrip())

                        if stripped_line.startswith('- #'):
                            tag_text = stripped_line[2:].strip()
                            tag_names = [tag_match.group(1) for tag_match in re.finditer(r'#([a-z0-9]+)', tag_text)]

                            if tag_names:
                                tag_stack = [(level, tag) for level, tag in tag_stack if level < indent_level]
                                for tag_name in tag_names:
                                    tag_stack.append((indent_level, tag_name))

                        elif stripped_line.startswith('- http://') or stripped_line.startswith('- https://'):
                            url_start_pos = 2
                            url_text = stripped_line[url_start_pos:].split()[0]
                            url = url_text.rstrip('.,;:!?')

                            parent_tags = [tag for level, tag in tag_stack if level < indent_level]

                            remaining_text = stripped_line[url_start_pos + len(url_text) :]
                            inline_tags = [
                                tag_match.group(1) for tag_match in re.finditer(r'#([a-z0-9]+)', remaining_text)
                            ]

                            combined_tags = parent_tags + inline_tags
                            if not combined_tags:
                                combined_tags = ['untagged']

                            url_tags[url] = (combined_tags, line_index + 1)
                        line_index += 1
                line_index += 1

        except OSError:
            pass

        return url_tags

    def fetch_url_metadata(self, url: str, timeout: int = 5) -> tuple[typing.Optional[str], str, int]:
        try:
            req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0 (compatible; qn-links/1.0)'})

            with urllib.request.urlopen(req, timeout=timeout) as response:
                status_code = response.getcode()
                content_type = response.headers.get('Content-Type', '')

                title = None
                if 'text/html' in content_type:
                    html_content = response.read(1_000_000).decode('utf-8', errors='ignore')
                    parser = TitleExtractor()
                    parser.feed(html_content)
                    title = parser.title.strip() if parser.title else None

                return title, 'active', status_code

        except urllib.error.HTTPError as e:
            return None, 'dead', e.code
        except (urllib.error.URLError, TimeoutError, Exception):
            return None, 'dead', 0

    def _scan_and_validate_urls(self) -> tuple[dict[str, tuple[str, int, list[str]]], dict[str, str]]:
        print('phase 1: validating tag blocks...')
        markdown_files = list(self.notes_dir.rglob('*.md'))

        all_tagged_urls: dict[str, tuple[str, int, list[str]]] = {}
        regular_urls: dict[str, str] = {}

        for file_path in markdown_files:
            relative_path = str(file_path.relative_to(self.notes_dir))

            tagged_urls = self.extract_tagged_urls_from_file(file_path)

            for url, (tags, line_num) in tagged_urls.items():
                if url in all_tagged_urls:
                    prev_file, prev_line, _ = all_tagged_urls[url]
                    raise ValueError(
                        f'duplicate URL found:\n'
                        f'{url}\n'
                        f'first occurrence: {prev_file}:{prev_line}\n'
                        f'duplicate found: {relative_path}:{line_num}\n'
                        f'each URL can only appear in one tag block'
                    )

                all_tagged_urls[url] = (relative_path, line_num, tags)

            all_urls = self.extract_urls_from_file(file_path)
            for url, line_num in all_urls:
                if url not in all_tagged_urls and url not in regular_urls:
                    regular_urls[url] = relative_path

        print(f'found {len(all_tagged_urls)} tagged links')
        print(f'found {len(regular_urls)} untagged links')
        print(f'scanned {len(markdown_files)} markdown files')

        return all_tagged_urls, regular_urls

    def _update_database(
        self, all_tagged_urls: dict[str, tuple[str, int, list[str]]], regular_urls: dict[str, str]
    ) -> None:
        print('\nphase 2: updating database...')

        current_urls: dict[str, str] = {}
        for url, (file_path, _, _) in all_tagged_urls.items():
            current_urls[url] = file_path
        for url, file_path in regular_urls.items():
            current_urls[url] = file_path

        conn = sqlite3.connect(self.db_path)
        try:
            cursor = conn.cursor()

            cursor.execute('SELECT url, first_seen FROM links')
            db_data = {row[0]: row[1] for row in cursor.fetchall()}

            orphaned_urls = set(db_data.keys()) - set(current_urls.keys())

            if orphaned_urls:
                print(f'removing {len(orphaned_urls)} orphaned links...')
                for url in orphaned_urls:
                    cursor.execute('DELETE FROM links WHERE url = ?', (url,))
                    cursor.execute('DELETE FROM link_tags WHERE url = ?', (url,))

            new_urls = set(current_urls.keys()) - set(db_data.keys())

            if new_urls:
                print(f'fetching metadata for {len(new_urls)} new links...')

                results = []
                with concurrent.futures.ThreadPoolExecutor(max_workers=self.max_workers) as executor:
                    future_to_url = {executor.submit(self.fetch_url_metadata, url): url for url in new_urls}

                    completed = 0
                    for future in concurrent.futures.as_completed(future_to_url):
                        url = future_to_url[future]
                        try:
                            title, status, status_code = future.result()
                            domain = urllib.parse.urlparse(url).netloc
                            now = datetime.datetime.now().isoformat()
                            url_file_path = current_urls[url]
                            results.append((url, title, status, status_code, now, domain, now, url_file_path))

                            completed += 1
                            if completed % 10 == 0:
                                print(f'processed {completed}/{len(new_urls)}...')
                        except Exception as e:
                            print(f'error processing {url}: {e}')

                for result in results:
                    cursor.execute(
                        """
                        INSERT INTO links (url, title, status, status_code, last_checked, domain, first_seen, file_path)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                        result,
                    )

            for url, url_file_path in current_urls.items():
                if url not in new_urls:
                    cursor.execute('UPDATE links SET file_path = ? WHERE url = ?', (url_file_path, url))

            print('updating tags...')
            cursor.execute('DELETE FROM link_tags')

            tag_counts: dict[str, int] = {}

            for url, (_, _, tags) in all_tagged_urls.items():
                unique_tags = set(tags)
                for tag in unique_tags:
                    cursor.execute('INSERT INTO link_tags (url, tag_name) VALUES (?, ?)', (url, tag))
                    tag_counts[tag] = tag_counts.get(tag, 0) + 1

            for url in regular_urls:
                cursor.execute('INSERT INTO link_tags (url, tag_name) VALUES (?, ?)', (url, 'untagged'))
                tag_counts['untagged'] = tag_counts.get('untagged', 0) + 1

            cursor.execute('DELETE FROM tags')
            for tag, count in tag_counts.items():
                cursor.execute('INSERT INTO tags (tag_name, link_count) VALUES (?, ?)', (tag, count))

            conn.commit()
        finally:
            conn.close()

        print(f'\n{len(new_urls)} new links added')
        print(f'{len(current_urls) - len(new_urls)} existing links updated')
        print(f'{len(tag_counts)} unique tags')
        print('\ncatalog complete')

    def catalog(self) -> None:
        print('scanning notes...')

        try:
            all_tagged_urls, regular_urls = self._scan_and_validate_urls()
        except ValueError as e:
            print(f'\nerror: {e}')
            print('no changes were made to the database')
            return

        self._update_database(all_tagged_urls, regular_urls)

    def audit(self) -> None:
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.cursor()

            print('checking link status...')
            cursor.execute('SELECT url, title, status_code, file_path FROM links WHERE status = "dead"')
            dead_links = cursor.fetchall()

        if dead_links:
            print(f'\nDEAD LINKS ({len(dead_links)} found):\n')

            for url, title, status_code, file_path in dead_links:
                print(f'{url} [{status_code} NOT FOUND]')
                if title:
                    print(f'  Title: "{title}"')
                if file_path:
                    print(f'  Found in: {file_path}')
                print()
        else:
            print('no dead links found')

    def check_duplicates(self) -> None:
        print('scanning notes for duplicate links...')

        url_locations: dict[str, list[tuple[str, int]]] = {}
        markdown_files = list(self.notes_dir.rglob('*.md'))

        for file_path in markdown_files:
            relative_path = str(file_path.relative_to(self.notes_dir))
            urls = self.extract_urls_from_file(file_path)

            for url, line_num in urls:
                if url not in url_locations:
                    url_locations[url] = []
                url_locations[url].append((relative_path, line_num))

        duplicates = {url: locations for url, locations in url_locations.items() if len(locations) > 1}

        if not duplicates:
            print('no duplicate links found')
            return

        print(f'\nDUPLICATE LINKS ({len(duplicates)} found):\n')

        for url in sorted(duplicates.keys()):
            locations = duplicates[url]
            print(f'{url}')
            print(f'found in {len(locations)} locations:')
            for rel_path, line_num in sorted(locations):
                print(f'  {rel_path}:{line_num}')
            print()

    def browse_urls(self, port: int, bind_address: str = '127.0.0.1') -> None:
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.cursor()

            query = """
                SELECT l.url, l.title, l.status, l.status_code, l.domain, l.first_seen,
                       GROUP_CONCAT(lt.tag_name, '|') as tags
                FROM links l
                LEFT JOIN link_tags lt ON l.url = lt.url
                GROUP BY l.url
                ORDER BY l.domain, l.url
            """

            cursor.execute(query)
            links = cursor.fetchall()

        urls_data = []
        for url, title, status, status_code, link_domain, first_seen, tags in links:
            tag_list = tags.split('|') if tags else []
            urls_data.append(
                {
                    'url': url,
                    'title': title,
                    'status': status,
                    'status_code': status_code,
                    'domain': link_domain,
                    'first_seen': first_seen,
                    'tags': tag_list,
                }
            )

        template_path = pathlib.Path(__file__).parent / 'qn_bookmark_browser.html'
        html_content = template_path.read_text()

        safe_json = json.dumps(urls_data, ensure_ascii=False, separators=(',', ':'))
        html_content = html_content.replace('{{URLS_DATA}}', safe_json)

        class RequestHandler(http.server.SimpleHTTPRequestHandler):
            def do_GET(self):
                if self.path not in ('/', '/index.html'):
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header('Content-type', 'text/html')
                self.end_headers()
                self.wfile.write(html_content.encode())

            def log_message(self, format, *args):
                pass

        class ReuseAddrTCPServer(socketserver.TCPServer):
            allow_reuse_address = True

        with ReuseAddrTCPServer((bind_address, port), RequestHandler) as httpd:
            url = f'http://{bind_address}:{port}'
            print(f'server listening on {bind_address}:{port}')
            print(f'opening browser at {url}')
            print('press ctrl+c to stop server')

            threading.Timer(1.0, lambda: webbrowser.open(url)).start()

            try:
                httpd.serve_forever()
            except KeyboardInterrupt:
                print('\nshutting down server...')
                httpd.shutdown()
                print('server stopped')


class Plugin:
    def __init__(self, ctx: dict):
        self.ctx = ctx
        config = ctx['config']

        try:
            max_workers = int(config.get('max_workers', '10'))
        except ValueError:
            max_workers = 10

        self.manager = LinkManager(pathlib.Path(ctx['notes_dir']), max_workers)

    def run(self) -> None:
        config = self.ctx['config']
        cmd = self.ctx['args'].get('url_command')

        match cmd:
            case 'catalog':
                self.manager.catalog()
            case 'audit':
                self.manager.audit()
            case 'dupes':
                self.manager.check_duplicates()
            case 'browse':
                try:
                    port = int(config.get('browser_port', '8765'))
                except ValueError:
                    port = 8765
                bind_address = config.get('bind_address', '127.0.0.1')
                self.manager.browse_urls(port=port, bind_address=bind_address)
            case _:
                print('usage: qn links {catalog,audit,dupes,browse}')
                print('  catalog  - scan notes and build/update link index')
                print('  audit    - check for dead links')
                print('  dupes    - check for duplicate links')
                print('  browse   - open web interface to browse URLs')


def register(parser) -> None:
    sub = parser.add_subparsers(dest='url_command', help='URL commands')
    sub.add_parser('catalog', help='scan notes and build/update link index')
    sub.add_parser('audit', help='check for dead links')
    sub.add_parser('dupes', help='check for duplicate links')
    sub.add_parser('browse', help='open web interface to browse URLs')


def run(ctx: dict) -> None:
    Plugin(ctx).run()


if __name__ == '__main__':
    import json
    import sys

    run(json.load(sys.stdin))
