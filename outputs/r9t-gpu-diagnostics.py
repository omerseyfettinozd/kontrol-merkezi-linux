#!/usr/bin/env python3
"""Read-only user-side GPU evidence collector."""
import json
from slayer_r9t.gpu_diagnostics import snapshot
print(json.dumps(snapshot(), ensure_ascii=False))
