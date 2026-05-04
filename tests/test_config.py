"""Unit tests for foretias.config."""

from __future__ import annotations

import os
import pathlib

import pytest

from foretias.config import Config


def _clear_foretias_home():
    """Ensure FORETIAS_HOME is not set."""
    os.environ.pop("FORETIAS_HOME", None)


class TestConfigResolve:
    def setup_method(self):
        _clear_foretias_home()

    def teardown_method(self):
        _clear_foretias_home()

    def test_default_path(self):
        cfg = Config.resolve()
        assert cfg.persist_path == str(pathlib.Path.home() / ".foretias")

    def test_env_var_overrides_default(self):
        os.environ["FORETIAS_HOME"] = "/tmp/test_foretias"
        cfg = Config.resolve()
        assert cfg.persist_path == "/tmp/test_foretias"

    def test_argument_overrides_env_var(self):
        os.environ["FORETIAS_HOME"] = "/tmp/env_path"
        cfg = Config.resolve(persist_path="/tmp/arg_path")
        assert cfg.persist_path == "/tmp/arg_path"

    def test_argument_overrides_default(self):
        cfg = Config.resolve(persist_path="/tmp/my_path")
        assert cfg.persist_path == "/tmp/my_path"

    def test_frozen(self):
        cfg = Config.resolve(persist_path="/tmp/test")
        with pytest.raises(AttributeError):
            cfg.persist_path = "/other"  # type: ignore[assignment]
