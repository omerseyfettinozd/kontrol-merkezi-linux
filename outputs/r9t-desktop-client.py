#!/usr/bin/env python3
import json,sys
from slayer_r9t.session import client
try:print(json.dumps(client(json.loads(sys.argv[1])),ensure_ascii=False))
except (OSError,ValueError,RuntimeError,IndexError) as exc:
    print(str(exc),file=sys.stderr);sys.exit(1)
