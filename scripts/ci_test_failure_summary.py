#!/usr/bin/env python3

from __future__ import annotations

import shlex
import sys
import xml.etree.ElementTree as ET
from pathlib import Path


def normalize_lines(text: str | None) -> list[str]:
    if not text:
        return []
    return [line.strip() for line in text.splitlines() if line.strip()]


def extract_summary(text: str | None) -> str:
    lines = normalize_lines(text)
    if not lines:
        return "Failure details were not captured."

    relevant: list[str] = []
    for line in lines:
        if line.startswith("stack backtrace:"):
            break
        if line.startswith("note: run with `RUST_BACKTRACE"):
            continue
        relevant.append(line)
        if len(relevant) == 6:
            break

    return " / ".join(relevant) if relevant else lines[0]


def rerun_command(profile: str, test_name: str) -> str:
    quoted_test_name = shlex.quote(test_name)
    if profile == "ci-all":
        return (
            "cargo nextest run --profile ci-all --all-features"
            f" -- {quoted_test_name} --exact"
        )
    if profile == "ci-no-default":
        return (
            "cargo nextest run --profile ci-no-default --no-default-features"
            f" -- {quoted_test_name} --exact"
        )
    return f"cargo nextest run --profile {profile} -- {quoted_test_name} --exact"


def collect_failures(report_path: Path) -> tuple[str, list[dict[str, str]]]:
    try:
        root = ET.parse(report_path).getroot()
    except ET.ParseError as exc:
        return report_path.stem, [
            {
                "name": f"{report_path.stem} (invalid JUnit XML)",
                "raw_test_name": "",
                "summary": f"Failed to parse JUnit XML: {exc}",
            }
        ]

    report_name = root.attrib.get("name", report_path.stem)
    failures: list[dict[str, str]] = []

    for suite in root.findall("testsuite"):
        suite_name = suite.attrib.get("name", "")
        for case in suite.findall("testcase"):
            failure = case.find("failure")
            if failure is None:
                failure = case.find("error")
            if failure is None:
                continue

            test_name = case.attrib.get("name", "<unknown>")
            full_name = f"{suite_name}::{test_name}" if suite_name else test_name
            failures.append(
                {
                    "name": full_name,
                    "raw_test_name": test_name,
                    "summary": extract_summary(failure.text),
                }
            )

    return report_name, failures


def profile_from_path(report_path: Path) -> str:
    parts = report_path.parts
    try:
        nextest_idx = parts.index("nextest")
        return parts[nextest_idx + 1]
    except (ValueError, IndexError):
        return "default"


def main() -> int:
    report_paths = [Path(arg) for arg in sys.argv[1:]]
    existing_reports = [path for path in report_paths if path.exists()]
    missing_reports = [path for path in report_paths if not path.exists()]

    print("## Test Failure Summary\n")

    if not existing_reports:
        print("No JUnit reports were found.")
        if missing_reports:
            print("")
            print("Missing JUnit report paths:")
            for report_path in missing_reports:
                print(f"- `{report_path}`")
        return 0

    if missing_reports:
        print("Warning: some requested JUnit reports were not found.\n")
        print("Missing JUnit report paths:")
        for report_path in missing_reports:
            print(f"- `{report_path}`")
        print("")

    total_failures = 0
    for report_path in existing_reports:
        profile = profile_from_path(report_path)
        report_name, failures = collect_failures(report_path)

        if not failures:
            continue

        total_failures += len(failures)
        print(f"### {profile}\n")
        print(f"Report: `{report_name}`\n")
        print("| Test | Panic Summary | Re-run |")
        print("| --- | --- | --- |")
        for failure in failures:
            test_name = failure["name"].replace("|", "\\|")
            summary = failure["summary"].replace("|", "\\|")
            rerun = (
                rerun_command(profile, failure["raw_test_name"])
                if failure["raw_test_name"]
                else "Unavailable"
            )
            print(f"| `{test_name}` | {summary} | `{rerun}` |")
        print("")

    if total_failures == 0:
        print("All reports completed without failed testcases.")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
