# shared helper module for backup/restore plugins, not a plugin itself
#

import os
import pathlib
import subprocess
import tempfile
import textwrap


class AgeEncryption:
    @staticmethod
    def generate_keypair() -> tuple[str, str]:
        try:
            result = subprocess.run(['age-keygen'], capture_output=True, text=True, check=True)
            lines = result.stdout.strip().split('\n')

            public_key = ''
            private_key = ''

            for line in lines:
                if line.startswith('# public key: '):
                    public_key = line.split(': ')[1]
                elif line.startswith('AGE-SECRET-KEY-'):
                    private_key = line.strip()
            if public_key and private_key:
                return public_key, private_key
            else:
                print('failed to parse age keypair output')
                return '', ''

        except (subprocess.CalledProcessError, FileNotFoundError, IndexError):
            print('failed to generate age keypair. age may not be installed.')
            return '', ''

    @staticmethod
    def encrypt_file(input_file: pathlib.Path, output_file: pathlib.Path, public_key: str) -> bool:
        try:
            subprocess.run(['age', '-r', public_key, '-o', str(output_file), str(input_file)], check=True)
            return True
        except subprocess.CalledProcessError:
            return False

    @staticmethod
    def decrypt_file(input_file: pathlib.Path, output_file: pathlib.Path, private_key: str) -> bool:
        with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as temp_key_file:
            temp_key_file.write(private_key)
            temp_key_path = pathlib.Path(temp_key_file.name)
        temp_key_path.chmod(0o600)

        try:
            subprocess.run(['age', '-d', '-i', str(temp_key_path), '-o', str(output_file), str(input_file)], check=True)
            return True
        except subprocess.CalledProcessError:
            return False
        finally:
            try:
                with open(temp_key_path, 'wb') as f:
                    f.write(b'\x00' * 1024)
                    f.flush()
                    os.fsync(f.fileno())
                temp_key_path.unlink()
            except OSError:
                pass


class GPGEncryption:
    @staticmethod
    def generate_keypair(name: str, email: str) -> str:
        try:
            gpg_commands = textwrap.dedent(f"""
                Key-Type: RSA
                Key-Length: 4096
                Subkey-Type: RSA
                Subkey-Length: 4096
                Name-Real: {name}
                Name-Email: {email}
                Expire-Date: 0
                %no-protection
                %commit
                """).strip()

            with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
                f.write(gpg_commands)
                temp_file = f.name

            result = subprocess.run(
                ['gpg', '--batch', '--gen-key', temp_file], capture_output=True, text=True, check=True
            )

            lines = result.stderr.split('\n')
            for line in lines:
                if 'key ' in line and 'created' in line:
                    key_id = line.split('key ')[1].split(' created')[0]
                    return key_id

            result = subprocess.run(['gpg', '--list-keys', '--with-colons'], capture_output=True, text=True, check=True)

            lines = result.stdout.split('\n')
            for line in reversed(lines):
                if line.startswith('pub:'):
                    parts = line.split(':')
                    if len(parts) > 4:
                        return parts[4]

            return ''

        except (subprocess.CalledProcessError, FileNotFoundError):
            print('failed to generate gpg keypair. gpg may not be installed.')
            return ''
        finally:
            if 'temp_file' in locals():
                pathlib.Path(temp_file).unlink(missing_ok=True)

    @staticmethod
    def encrypt_file(input_file: pathlib.Path, output_file: pathlib.Path, recipient: str = '') -> bool:
        try:
            if recipient:
                subprocess.run(
                    ['gpg', '--encrypt', '--recipient', recipient, '--output', str(output_file), str(input_file)],
                    check=True,
                )
            else:
                subprocess.run(
                    ['gpg', '--symmetric', '--cipher-algo', 'AES256', '--output', str(output_file), str(input_file)],
                    check=True,
                )
            return True
        except subprocess.CalledProcessError:
            return False

    @staticmethod
    def decrypt_file(input_file: pathlib.Path, output_file: pathlib.Path) -> bool:
        try:
            subprocess.run(['gpg', '--decrypt', '--output', str(output_file), str(input_file)], check=True)
            return True
        except subprocess.CalledProcessError:
            return False
