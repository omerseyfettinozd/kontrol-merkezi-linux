#!/usr/bin/env python3
"""Unprivileged Dynamic Boost status collector; no GPU management calls."""
import json
from slayer_r9t.dynamic_boost import snapshot
print(json.dumps(snapshot(),ensure_ascii=False))
