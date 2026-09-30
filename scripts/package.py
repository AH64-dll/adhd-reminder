#!/usr/bin/env python3
"""Package already-built Linux and Windows binaries, installer scripts and license notices."""
import hashlib
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import zipfile

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--linux-binary', type=Path, default=root / 'target/release/adhd')
args = parser.parse_args()
dist = root / 'dist'
dist.mkdir(exist_ok=True)
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1'], cwd=root))

def bundle(platform, binary):
    if not binary.is_file(): raise SystemExit(f'Missing binary: {binary}')
    folder = dist / f'adhd-0.1.0-{platform}-x86_64'
    (folder / 'bin').mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, folder / 'bin' / binary.name)
    for name in ['README.md', 'LICENSE', 'THIRD_PARTY.md', 'VERIFICATION.md']:
        if (root / name).is_file(): shutil.copy2(root / name, folder / name)
    shutil.copytree(root / 'scripts', folder / 'scripts', dirs_exist_ok=True, ignore=shutil.ignore_patterns('__pycache__'))
    shutil.copytree(root / 'integrations', folder / 'integrations', dirs_exist_ok=True)
    licenses = folder / 'licenses'
    licenses.mkdir(exist_ok=True)
    packages = []
    for package in metadata['packages']:
        packages.append({k: package.get(k) for k in ['name', 'version', 'license', 'repository']})
        source = Path(package['manifest_path']).parent
        destination = licenses / f"{package['name']}-{package['version']}"
        for item in source.iterdir():
            if item.name.lower().startswith(('license', 'licence', 'copying', 'notice', 'copyright')):
                destination.mkdir(exist_ok=True)
                if item.is_dir(): shutil.copytree(item, destination / item.name, dirs_exist_ok=True)
                elif item.is_file(): shutil.copy2(item, destination / item.name)
        if package['name'] == 'slint':
            shutil.copy2(source / 'LICENSES/LicenseRef-Slint-Royalty-free-2.0.md', licenses / 'Slint.txt')
    (licenses / 'dependencies.json').write_text(json.dumps(packages, indent=2))
    if platform == 'windows':
        archive = Path(str(folder) + '.zip')
        # Cargo archives may give license files a Unix-epoch timestamp. ZIP starts in
        # 1980, so clamp source timestamps without changing the copied license files.
        with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED, strict_timestamps=False) as output:
            for item in sorted(folder.rglob('*')):
                if item.is_file(): output.write(item, item.relative_to(dist))
    else:
        archive = Path(shutil.make_archive(str(folder), 'gztar', root_dir=dist, base_dir=folder.name))
    return archive, hashlib.sha256(archive.read_bytes()).hexdigest()

archives = [bundle('linux', args.linux_binary.resolve()), bundle('windows', root / 'target/x86_64-pc-windows-gnu/release/adhd.exe')]
(dist / 'SHA256SUMS').write_text(''.join(f'{checksum}  {path.name}\n' for path, checksum in archives))
for path, checksum in archives: print(f'{path} ({path.stat().st_size / 1024 / 1024:.1f} MiB) sha256:{checksum}')
