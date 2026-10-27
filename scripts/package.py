#!/usr/bin/env python3
"""Package the prebuilt aarch64 binary for yam install."""
from pathlib import Path
import argparse
import struct
import tomllib
import zipfile

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, default=root / 'wakeword-cli.yap')
args = parser.parse_args()
version = tomllib.loads((root / 'wakeword-cli/Cargo.toml').read_text())['package']['version']
binary = root / 'target/aarch64-unknown-linux-musl/release/wakeword-cli'
manifest = f'''id = "wakeword-cli"
name = "Wakeword CLI"
version = "{version}"
entry = "wakeword"
description = "Переключение слова активации: Алиса / Яндекс / Ясмина. CLI и JSON API v1."
permissions = ["exec"]

[launch]
kind = "binary"
path = "wakeword"

'''
output = args.output
output.parent.mkdir(parents=True, exist_ok=True)
binary_data = binary.read_bytes()
if binary_data[:4] != b'\x7fELF' or binary_data[4] != 2 or struct.unpack_from('<H', binary_data, 18)[0] != 183:
    raise SystemExit('Expected an ELF64 AArch64 binary; build aarch64-unknown-linux-musl first')
with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED) as archive:
    archive.writestr('plugin.toml', manifest)
    entry = zipfile.ZipInfo('wakeword')
    entry.create_system = 3
    entry.external_attr = 0o100755 << 16
    archive.writestr(entry, binary_data, compress_type=zipfile.ZIP_DEFLATED)
print(output)
