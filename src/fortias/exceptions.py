"""Fortias Protocol v0.0.1 — Exceptions."""

from __future__ import annotations


class FortiasError(Exception):
    """Base class for all Fortias-raised errors."""


class InvalidKeyError(FortiasError):
    """Raised when a signing key is missing, malformed, or of the wrong length."""


class PayloadHashMismatchError(FortiasError):
    """Raised when a request's ``stamp_request_hash`` does not match
    the SHA-256 digest of its ``payload``.

    This is a pre-check run by both /stamp and /verify *before* any further
    processing; it protects against an on-the-wire payload swap in which
    the hash the client claims and the payload the client sends disagree.
    """
