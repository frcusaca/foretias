"""Allow running foretias as ``python -m foretias``."""

from __future__ import annotations

import sys

from .cli import main

sys.exit(main())
