#!/usr/bin/env python3
"""Print a removal inventory; this command cannot uninstall anything."""
import argparse
import json
from pathlib import Path
from slayer_r9t.uninstall_plan import inventory


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dry-run', action='store_true', required=True)
    parser.parse_args(argv)
    print(json.dumps(inventory(Path(__file__).resolve().parent), ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
