#!/usr/bin/env bash
set -euo pipefail

exec ruby "$(dirname "$0")/lint_test_names.rb"
