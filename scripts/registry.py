#!/usr/bin/env python3
"""Generate the YAM registry entry from the exact archive uploaded to GitHub Releases."""
import argparse
import hashlib
import json
from pathlib import Path
import tomllib
import zipfile


def build(package):
    with zipfile.ZipFile(package) as archive:
        manifest = tomllib.loads(archive.read('plugin.toml').decode())
    package_id = manifest['id']
    version = manifest['version']
    compatibility = manifest.get('compatibility', {})
    entry = {
        'id': package_id,
        'version': version,
        'name': manifest['name'],
        'description': manifest.get('description', ''),
        'sha256': hashlib.sha256(package.read_bytes()).hexdigest(),
        'models': compatibility.get('models', []),
        'features': compatibility.get('features', []),
        'target': 'any',
        'url': f'https://github.com/nayvok/wakeword/releases/download/v{version}/wakeword-cli.yap',
    }
    return {'format': 3, 'packages': [entry], 'latest': {package_id: version}}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('package', type=Path)
    parser.add_argument('--output', type=Path, default=Path('registry/index.json'))
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(build(args.package), ensure_ascii=False, indent=2) + '\n')
    print(args.output)
