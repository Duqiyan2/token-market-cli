#!/usr/bin/env python3
"""Hold a CLI-published fixture store open for the native client integration test."""
from pathlib import Path
import json
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from test_update_distribution import DistributionCommand

fixture = DistributionCommand()
fixture.setUp()
try:
    with fixture.served_store() as (store, base):
        result = fixture.publish(store, base)
        if result.returncode:
            sys.stderr.write(result.stderr)
            sys.exit(result.returncode)
        print(json.dumps({'endpoint': base + '/test/candidate/stable.json'}), flush=True)
        sys.stdin.readline()
finally:
    fixture.doCleanups()
