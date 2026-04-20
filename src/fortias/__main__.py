"""Allow running fortias as ``python -m fortias``."""

from __future__ import annotations

import sys

from .cli import main

sys.exit(main())
