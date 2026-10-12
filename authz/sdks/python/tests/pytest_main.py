"""Runs this directory's tests with pytest, for Bazel's py_test."""

import os
import sys

import pytest

if __name__ == "__main__":
    sys.exit(pytest.main([os.path.dirname(os.path.abspath(__file__)), "-q", "-p", "no:cacheprovider"]))
