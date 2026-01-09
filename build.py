#!/usr/bin/env python3
import argparse
import dataclasses
import hashlib
import pathlib
import subprocess
import sys
import tarfile


class CommandError(Exception):
    pass


@dataclasses.dataclass(frozen=True)
class BuildTarget:
    tool: str
    target: str
    asset_name: str
    dockerfile: str | None = None


TARGETS: list[BuildTarget] = [
    BuildTarget(
        tool='dotsync',
        target='aarch64-apple-darwin',
        asset_name='dotsync-macos-aarch64',
    ),
    BuildTarget(
        tool='dotsync',
        target='x86_64-unknown-linux-musl',
        asset_name='dotsync-linux-x86_64',
        dockerfile='Dockerfile.linux-x86_64',
    ),
    BuildTarget(
        tool='dotsync',
        target='aarch64-unknown-linux-musl',
        asset_name='dotsync-linux-aarch64',
        dockerfile='Dockerfile.linux-aarch64',
    ),
]


def run_command(cmd: list[str], cwd: pathlib.Path | None = None) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if result.returncode != 0:
        error_msg = f'command failed: {" ".join(cmd)}\nstderr: {result.stderr}'
        raise CommandError(error_msg)
    return result


def run_tests(tool_dir: pathlib.Path) -> bool:
    print(f'running tests for {tool_dir.name}')
    result = subprocess.run(
        ['cargo', 'test', '--all-features'],
        cwd=tool_dir,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(f'tests failed: {result.stderr}')
        return False
    print('tests passed')
    return True


def build_native(tool_dir: pathlib.Path, target: BuildTarget) -> pathlib.Path:
    print(f'building native binary for {target.target}')
    run_command(['cargo', 'build', '--release', '--target', target.target], cwd=tool_dir)
    binary_path = tool_dir / 'target' / target.target / 'release' / target.tool
    run_command(['strip', str(binary_path)])
    dest_path = tool_dir / target.tool
    run_command(['cp', str(binary_path), str(dest_path)])
    return dest_path


def build_docker_image(tool_dir: pathlib.Path, target: BuildTarget) -> str:
    if not target.dockerfile:
        raise ValueError(f'no dockerfile specified for {target.target}')
    image_tag = f'{target.tool}-builder:{target.target}'
    print(f'building docker image: {image_tag}')
    run_command(
        [
            'docker',
            'build',
            '-f',
            target.dockerfile,
            '-t',
            image_tag,
            '.',
        ],
        cwd=tool_dir,
    )
    return image_tag


def extract_binary(tool_dir: pathlib.Path, target: BuildTarget, image_tag: str) -> pathlib.Path:
    container_name = f'temp-{target.tool}-{target.target}'
    print(f'extracting binary from {image_tag}')
    run_command(['docker', 'create', '--name', container_name, image_tag])
    binary_path = tool_dir / target.tool
    run_command(
        ['docker', 'cp', f'{container_name}:/build/target/{target.target}/release/{target.tool}', str(binary_path)]
    )
    run_command(['docker', 'rm', container_name])
    return binary_path


def create_archive(tool_dir: pathlib.Path, target: BuildTarget, output_dir: pathlib.Path) -> pathlib.Path:
    binary_path = tool_dir / target.tool
    archive_path = output_dir / f'{target.asset_name}.tar.gz'
    print(f'creating archive: {archive_path.name}')
    with tarfile.open(archive_path, 'w:gz') as tar:
        tar.add(binary_path, arcname=target.tool)
    binary_path.unlink()
    return archive_path


def create_checksum(archive_path: pathlib.Path) -> pathlib.Path:
    checksum_path = archive_path.parent / (archive_path.name + '.sha256')
    sha256_hash = hashlib.sha256()
    with open(archive_path, 'rb') as f:
        for chunk in iter(lambda: f.read(8192), b''):
            sha256_hash.update(chunk)
    checksum = sha256_hash.hexdigest()
    checksum_path.write_text(f'{checksum}  {archive_path.name}\n')
    print(f'checksum: {checksum}')
    return checksum_path


def build_target(repo_root: pathlib.Path, target: BuildTarget, output_dir: pathlib.Path) -> list[pathlib.Path]:
    tool_dir = repo_root / target.tool
    print(f'building {target.asset_name}')

    if target.dockerfile:
        image_tag = build_docker_image(tool_dir, target)
        extract_binary(tool_dir, target, image_tag)
    else:
        build_native(tool_dir, target)

    archive_path = create_archive(tool_dir, target, output_dir)
    checksum_path = create_checksum(archive_path)
    return [archive_path, checksum_path]


def get_tools_from_targets() -> list[str]:
    return list(set(t.tool for t in TARGETS))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description='build rust tools for multiple platforms')
    parser.add_argument(
        '--tool',
        type=str,
        choices=get_tools_from_targets(),
        help='build only a specific tool',
    )
    parser.add_argument(
        '--target',
        type=str,
        help='build only a specific target (e.g., x86_64-unknown-linux-musl)',
    )
    parser.add_argument(
        '--output-dir',
        type=pathlib.Path,
        default=pathlib.Path('dist'),
        help='output directory for artifacts',
    )
    parser.add_argument(
        '--skip-tests',
        action='store_true',
        help='skip running tests',
    )
    parser.add_argument(
        '--list-targets',
        action='store_true',
        help='list all available build targets',
    )
    return parser.parse_args()


def filter_targets(tool: str | None, target: str | None) -> list[BuildTarget]:
    filtered = TARGETS
    if tool:
        filtered = [t for t in filtered if t.tool == tool]
    if target:
        filtered = [t for t in filtered if t.target == target]
    return filtered


def main() -> int:
    args = parse_args()
    if args.list_targets:
        for t in TARGETS:
            print(f'{t.tool}: {t.target} -> {t.asset_name}')
        return 0

    repo_root = pathlib.Path(__file__).parent.resolve()
    output_dir: pathlib.Path = args.output_dir
    if not output_dir.is_absolute():
        output_dir = repo_root / output_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    targets = filter_targets(args.tool, args.target)
    if not targets:
        print('no matching targets found')
        return 1

    if not args.skip_tests:
        tools_to_test = set(t.tool for t in targets)
        for tool in tools_to_test:
            if not run_tests(repo_root / tool):
                return 1

    artifacts: list[pathlib.Path] = []
    try:
        for target in targets:
            artifacts.extend(build_target(repo_root, target, output_dir))
    except CommandError as e:
        print(f'error: {e}')
        return 1

    print(f'build complete. artifacts in {output_dir}:')
    for artifact in artifacts:
        print(f'  {artifact.name}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
