"""Unit tests for fortias.config."""

from __future__ import annotations

import os
import pathlib

import pytest

from fortias.config import Config


def _clear_fortias_home():
    """Ensure FORTIAS_HOME is not set."""
    os.environ.pop("FORTIAS_HOME", None)


class TestConfigResolve:
    def setup_method(self):
        _clear_fortias_home()

    def teardown_method(self):
        _clear_fortias_home()

    def test_default_path(self):
        cfg = Config.resolve()
        assert cfg.persist_path == str(pathlib.Path.home() / ".fortias")

    def test_env_var_overrides_default(self):
        os.environ["FORTIAS_HOME"] = "/tmp/test_fortias"
        cfg = Config.resolve()
        assert cfg.persist_path == "/tmp/test_fortias"

    def test_argument_overrides_env_var(self):
        os.environ["FORTIAS_HOME"] = "/tmp/env_path"
        cfg = Config.resolve(persist_path="/tmp/arg_path")
        assert cfg.persist_path == "/tmp/arg_path"

    def test_argument_overrides_default(self):
        cfg = Config.resolve(persist_path="/tmp/my_path")
        assert cfg.persist_path == "/tmp/my_path"

    def test_frozen(self):
        cfg = Config.resolve(persist_path="/tmp/test")
        with pytest.raises(AttributeError):
            cfg.persist_path = "/other"  # type: ignore[assignment]
