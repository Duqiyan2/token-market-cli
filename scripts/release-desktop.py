#!/usr/bin/env python3
"""Current desktop release entry: build, stage, publish, verify, and repair OSS metadata."""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True
import desktop_release_managed as release


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ('doctor', 'build', 'stage', 'publish', 'check', 'metadata', 'release'):
        command = commands.add_parser(name)
        command.add_argument('--config', required=True)
        if name in ('build', 'release'):
            command.add_argument('--source')
            command.add_argument('--version')
            command.add_argument('--windows-dir', help='Use finalized native Windows output instead of cross-building')
        if name == 'build':
            command.add_argument('--target', choices=['all', 'mac', 'windows'], default='all')
        if name == 'stage':
            command.add_argument('--mac-dir', required=True)
            command.add_argument('--windows-dir', required=True)
            command.add_argument('--output', required=True)
        if name in ('stage', 'release'):
            command.add_argument('--notes-file')
        if name in ('publish', 'check', 'metadata'):
            command.add_argument('--plan', required=True)
        if name == 'metadata':
            command.add_argument('--key', action='append')
        if name in ('stage', 'publish', 'check', 'metadata'):
            command.add_argument('--fixture', action='store_true', help=argparse.SUPPRESS)
            command.add_argument('--local-store', help=argparse.SUPPRESS)
    args = parser.parse_args()
    args.fixture = getattr(args, 'fixture', False)
    args.local_store = getattr(args, 'local_store', None)
    try:
        release.require(not args.local_store or args.fixture, '--local-store requires --fixture')
        release.require(not args.fixture or args.local_store, '--fixture requires an isolated --local-store')
        config = release.config_at(args.config, args.fixture)
        if args.command == 'doctor':
            cache = Path(config['_config_path']).parent / 'build-cache'
            env = release.compiler_environment(config, cache)
            tools = {key: bool(shutil.which(key, path=env['PATH'])) for key in ('cargo', 'cargo-xwin', 'llvm-rc', 'lld-link', 'makensis', '7zz')}
            files = {key: bool(config.get(key) and Path(config[key]).is_file()) for key in ('public_key_file', 'private_key_file')}
            files.update({key: bool(config['tools'].get(key) and Path(config['tools'][key]).is_file()) for key in ('signer', 'verifier')})
            files['macos_identity'] = all((Path(config['native']['macos_identity_dir']) / name).is_file()
                for name in ('certificate.pem', 'keychain-password', 'signing.keychain-db', 'identity.json'))
            release.credentials(config, False)
            print(json.dumps(dict(command='doctor', tools=tools, configured_files=files, oss_profile_configured=True, config_outside_git=True)))
            return 0 if all(tools.values()) and all(files.values()) else 1
        if args.command == 'stage':
            plan = release.stage(args, config)
            print(json.dumps(dict(staged=True, plan=str(plan))))
            return 0
        if args.command in ('build', 'release'):
            mac, windows = release.build(args, config)
            if args.command == 'build':
                print(json.dumps(dict(built=True, mac_dir=str(mac) if mac else None, windows_dir=str(windows) if windows else None)))
                return 0
            args.mac_dir, args.windows_dir = str(mac), str(windows)
            args.output = str(mac.parent / 'release')
            args.plan = str(release.stage(args, config))
            args.command = 'publish'
        plan = release.load_plan(args, config)
        if args.command == 'check':
            release.check(config, plan)
        else:
            release.publish(args, config, plan)
            # Always verify the complete public release after any write command.
            release.check(config, plan)
        result = dict(passed=True, action=args.command, version=plan['version'], fixture=args.fixture,
            checked_at=datetime.now(timezone.utc).isoformat(), windows_authenticode_signed=plan.get('windows_authenticode_signed', False),
            native_device_validated=plan.get('native_device_validated', False))
        Path(args.plan).with_name('managed-release-result.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(result))
    except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print('Release stopped: ' + str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
