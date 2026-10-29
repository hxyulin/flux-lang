#!/usr/bin/env python3
"""Select Cargo test targets, build once, and schedule bounded test binaries."""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
import json
import os
import platform
import re
import signal
from pathlib import Path
import subprocess
import sys
import time
import threading

ROOT = Path(__file__).resolve().parents[1]
GROUPS = ("all", "syntax", "names", "inference", "borrowing", "codegen", "library", "graphs", "units")


@dataclass(frozen=True, order=True)
class Target:
    package: str
    name: str
    kind: str
    source: str
    package_id: str = ""

    @property
    def label(self) -> str:
        return f"{self.package}::{self.name}"

    @property
    def exclusive(self) -> bool:
        return graph(self) or self.name in {"run", "proof_compatibility"} or "scaling" in self.name


@dataclass(frozen=True)
class Smoke:
    filter: str | None = None
    case: str | None = None
    native: bool = False
    expected_tests: int = 1


# Small acceptance/rejection samples, not complete component coverage. Keep case
# names explicit so additions to a fixture directory cannot expand the fast path.
FAST_COMPONENTS = {
    "syntax": {
        ("flux-syntax", "parser"): Smoke("fixtures", "expressions.rs,patterns.rs", expected_tests=2),
        ("flux-sourcegen", "ast"): Smoke("generated_ast_is_fresh"),
    },
    "names": {
        ("flux-hir", "hir"): Smoke("fixtures", "functions.rs,modules.rs,imports.rs", expected_tests=2),
    },
    "inference": {
        ("flux-ty", "ty"): Smoke("fixtures", "methods.rs,coercion_sites.rs", expected_tests=2),
    },
    "borrowing": {
        ("flux-mir", "mir"): Smoke("fixtures", "moves.rs,consts.rs", expected_tests=2),
    },
    "codegen": {
        ("flux-codegen", "targets"): Smoke("supported_object_formats_and_unimplemented_linux_target"),
        ("flux-cli", "run"): Smoke("programs_match_rustc", "aggregate_storage,const_function_pointers", native=True),
    },
    "library": {
        ("flux-ty", "std"): Smoke(expected_tests=4),
        ("flux-cli", "test_support"): Smoke(expected_tests=2),
        ("flux-cli", "native_fixtures"): Smoke("cow_as_ref", native=True),
        ("flux-cli", "c_strings"): Smoke("c_strings_match_rustc", native=True),
    },
}


def fast_plan(available: list[Target], group: str, native: bool | None = None) -> dict[Target, Smoke]:
    if group == "graphs":
        raise RuntimeError("source-graph audits have no smoke mode; run graphs without --fast")
    if native is None:
        native = sys.platform == "darwin" and platform.machine() in {"arm64", "aarch64"}
    selected = {t: Smoke() for t in available if t.kind != "test" and in_group(t, group)}
    groups = FAST_COMPONENTS if group == "all" else ([group] if group != "units" else [])
    by_name = {(t.package, t.name): t for t in available if t.kind == "test"}
    for component in groups:
        for key, smoke in FAST_COMPONENTS[component].items():
            if smoke.native and not native:
                continue
            if key not in by_name:
                raise RuntimeError(f"missing fast target {key[0]}::{key[1]}; update FAST_COMPONENTS")
            selected[by_name[key]] = smoke
    return dict(sorted(selected.items()))


def job_settings(target: Target, args: argparse.Namespace, smokes: dict[Target, Smoke]) -> argparse.Namespace:
    if not args.fast:
        return args
    smoke = smokes[target]
    return argparse.Namespace(**{**vars(args),
        "filter": args.filter if args.filter is not None else smoke.filter,
        "case": args.case if args.case is not None else smoke.case,
        "expected_tests": smoke.expected_tests if target.kind == "test" and args.filter is None else None})


def owns_native_sweep(selected: list[Target], args: argparse.Namespace) -> bool:
    # A fast runtime sample cannot take ownership of unrelated native fixtures.
    return (not args.fast and not args.case and not args.filter
            and any(t.package == "flux-cli" and t.name == "run" for t in selected))


def graph(target: Target) -> bool:
    return target.package == "flux-cli" and (
        "selfhost" in target.name or "readiness" in target.name
        or target.name in {"keel_float_math", "keel_reduced_suite", "path_slice_pool"}
    )


def targets(metadata: dict) -> list[Target]:
    members = set(metadata["workspace_members"])
    return sorted(
        Target(p["name"], t["name"], t["kind"][0], t["src_path"], p["id"])
        for p in metadata["packages"] if p["id"] in members
        for t in p["targets"] if t["test"] and t["kind"][0] in {"test", "lib", "bin"}
    )


def in_group(target: Target, group: str) -> bool:
    if group == "all":
        return True
    if group == "units":
        return target.kind != "test"
    if group == "graphs":
        return graph(target)
    packages = {
        "syntax": {"flux-syntax", "flux-sourcegen"},
        "names": {"flux-hir"},
        "inference": {"flux-ty"},
        "borrowing": {"flux-mir"},
        "codegen": {"flux-codegen", "flux-cli"},
        "library": {"flux-cli"},
    }
    return target.package in packages[group] and not graph(target)


def changed_paths(base: str) -> list[str]:
    # Include both sides of renames, deletions, unstaged changes, and new files.
    tracked = subprocess.check_output(
        ["git", "diff", "--no-renames", "--name-only", "-z", base, "--"], cwd=ROOT
    ).decode().split("\0")
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard", "-z"], cwd=ROOT
    ).decode().split("\0")
    return sorted(set(tracked + untracked) - {""})


def affected(metadata: dict, paths: list[str], available: list[Target]) -> set[Target]:
    """Production edits select the reverse dependency closure; unknown paths select all."""
    members = set(metadata["workspace_members"])
    packages = {p["name"]: p for p in metadata["packages"] if p["id"] in members}
    roots = {name: Path(p["manifest_path"]).parent.relative_to(ROOT).as_posix()
             for name, p in packages.items()}
    selected: set[Target] = set()
    changed_packages: set[str] = set()
    for path in paths:
        if path.startswith("tests/run/"):
            selected.update(t for t in available if t.package == "flux-cli")
            continue
        owner = next((name for name, root in roots.items() if path.startswith(root + "/")), None)
        if owner is None:
            return set(available)
        relative = path[len(roots[owner]) + 1:]
        if relative.startswith("tests/"):
            candidate = relative.split("/")[1].removesuffix(".rs")
            exact = [t for t in available if t.package == owner and t.name == candidate]
            # Shared support, fixtures and removed/renamed targets broaden to the package.
            if exact:
                selected.update(exact)
            else:
                # Other crates import fixture/support files with #[path] and include_str!.
                changed_packages.add(owner)
        else:
            changed_packages.add(owner)
    while True:
        expanded = changed_packages | {
            name for name, p in packages.items()
            if any(dep["name"] in changed_packages for dep in p["dependencies"])
        }
        if expanded == changed_packages:
            break
        changed_packages = expanded
    selected.update(t for t in available if t.package in changed_packages)
    return selected


def build(cargo: str, selected: list[Target], directory: Path, complete: bool = False) -> dict[Target, Path]:
    executables: dict[Target, Path] = {}
    batches = [("workspace", selected)] if complete else [
        (package, [t for t in selected if t.package == package])
        for package in sorted({t.package for t in selected})
    ]
    for package, package_targets in batches:
        command = [cargo, "test", "--no-run", "--message-format=json"]
        if complete:
            # Preserve Cargo's normal example-build checks as well as test targets.
            command += ["--workspace"]
        else:
            command += ["-p", package]
            for target in package_targets:
                command += ["--test", target.name] if target.kind == "test" else ["--" + ("lib" if target.kind == "lib" else "bin")]
                if target.kind == "bin":
                    command.append(target.name)
        print(f"Building {package} ({len(package_targets)} targets)", flush=True)
        result = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, text=True)
        (directory / f"build-{package}.jsonl").write_text(result.stdout)
        if result.returncode:
            raise RuntimeError(f"build failed: {package}")
        for line in result.stdout.splitlines():
            try:
                artifact = json.loads(line)
            except json.JSONDecodeError:
                continue  # Local Cargo wrappers may print their own status lines.
            if artifact.get("reason") != "compiler-artifact" or not artifact.get("executable"):
                continue
            if not artifact["profile"]["test"]:
                continue
            for target in package_targets:
                if (artifact["target"]["name"] == target.name
                        and target.kind in artifact["target"]["kind"]
                        and artifact["package_id"] == target.package_id
                        and Path(artifact["target"]["src_path"]) == Path(target.source)):
                    executables[target] = Path(artifact["executable"])
        missing = set(package_targets) - executables.keys()
        if missing:
            raise RuntimeError(f"missing test executables: {missing}")
    return executables


class RunControl:
    """Cancel active process groups and prevent queued jobs from starting."""

    def __init__(self):
        self.lock = threading.RLock()
        self.children: set[subprocess.Popen] = set()
        self.stopped = threading.Event()

    def start(self, command: list[str], **options) -> subprocess.Popen | None:
        with self.lock:
            if self.stopped.is_set():
                return None
            child = subprocess.Popen(command, start_new_session=True, **options)
            self.children.add(child)
            # A signal can cancel an exclusive job on this same thread while
            # Popen is starting. Recheck after registering its process group.
            if self.stopped.is_set():
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            return child

    def finish(self, child: subprocess.Popen):
        with self.lock:
            self.children.discard(child)

    def cancel(self):
        self.stopped.set()
        with self.lock:
            for child in self.children:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass


def run_target(target: Target, executable: Path, args: argparse.Namespace,
               directory: Path, deduplicate: bool, control: RunControl | None = None) -> dict:
    control = control or RunControl()
    start = time.monotonic()
    log = directory / f"{target.label.replace('::', '--')}.log"
    threads = args.jobs if target.name == "proof_compatibility" else 1
    env = os.environ.copy()
    # This command's selection is explicit; do not inherit stale filters/ownership.
    for key in ("FLUX_TEST_CASE", "FLUX_TEST_NATIVE_SWEEP", "FLUX_TEST_JOBS"):
        env.pop(key, None)
    env["FLUX_TEST_JOBS"] = str(args.jobs if target.name == "run" else 1)
    if args.case:
        env["FLUX_TEST_CASE"] = args.case
    if deduplicate:
        env["FLUX_TEST_NATIVE_SWEEP"] = "1"
    command = [str(executable), "--nocapture", f"--test-threads={threads}"]
    if args.filter:
        command.append(args.filter)
    print(f"Running {target.label}", flush=True)
    timed_out = False
    with log.open("w") as output:
        child = control.start(command, cwd=Path(target.source).parent.parent, env=env,
                              stdout=output, stderr=subprocess.STDOUT)
        status = 130
        if child is not None:
            try:
                status = child.wait(timeout=args.timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                status = child.wait()
            finally:
                control.finish(child)
    if status == 0 and getattr(args, "fast", False):
        output = log.read_text()
        reason = None
        count = re.search(r"(?m)^running (\d+) tests?$", output)
        if target.kind == "test":
            if count is None or int(count[1]) == 0:
                reason = "smoke filter selected no test functions"
            elif getattr(args, "expected_tests", None) is not None and int(count[1]) != args.expected_tests:
                reason = f"smoke selected {count[1]} test functions, expected {args.expected_tests}; update the smoke plan"
        if args.case:
            matched = re.findall(r"flux-case: (.+)", output)
            if any(not any(pattern.strip() in name for name in matched)
                   for pattern in args.case.split(',')):
                reason = "smoke case filter did not match every configured pattern"
        if reason:
            status = 1
            with log.open("a") as output:
                output.write(f"\nERROR: {reason}\n")
            print(f"{target.label}: {reason}; see {log}", file=sys.stderr)
    record = {"target": target.label, "source": target.source, "executable": str(executable),
              "filter": args.filter, "case": args.case,
              "exit_code": status, "timed_out": timed_out,
              "seconds": round(time.monotonic() - start, 2), "log": str(log)}
    print(f"{target.label}: {'PASS' if status == 0 else 'FAIL'} ({record['seconds']}s)", flush=True)
    return record


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("group", nargs="?", choices=("fast", *GROUPS), default="all")
    parser.add_argument("--fast", action="store_true", help="run the component smoke plan, excluding exhaustive suites and source graphs")
    parser.add_argument("--changed", nargs="?", const="HEAD", metavar="BASE", help="select tests affected by changes since BASE (default HEAD)")
    parser.add_argument("--package", action="append", default=[], help="restrict to a package; repeatable")
    parser.add_argument("--target", action="append", default=[], help="restrict to a test binary; repeatable")
    parser.add_argument("--case", help="comma-separated case substrings for fixture/oracle loops")
    parser.add_argument("--filter", help="libtest function-name substring")
    parser.add_argument("--jobs", type=int, default=min(4, os.cpu_count() or 1))
    parser.add_argument("--timeout", type=int, help="per-binary deadline in seconds, including descendants (fast: 30; otherwise: 1800)")
    parser.add_argument("--cargo", default="cargo", help="Cargo command (local builds may use mbx)")
    parser.add_argument("--list", action="store_true", help="print the plan without compiling or running")
    args = parser.parse_args()
    if args.group == "fast":
        args.group, args.fast = "all", True
    if args.timeout is None:
        args.timeout = 30 if args.fast else 1800
    if args.fast and (args.filter or args.case) and not args.target:
        parser.error("fast filter overrides require --target")
    if args.jobs < 1 or args.timeout < 1 or args.case is not None and any(not p.strip() for p in args.case.split(",")):
        parser.error("jobs/timeouts must be positive and case patterns must not be empty")
    metadata = json.loads(subprocess.check_output(
        [args.cargo, "metadata", "--no-deps", "--format-version=1"], cwd=ROOT
    ))
    available = targets(metadata)
    smokes = fast_plan(available, args.group) if args.fast else {}
    selected = list(smokes) if args.fast else [t for t in available if in_group(t, args.group)]
    changed = []
    if args.changed is not None:
        changed = changed_paths(args.changed)
        print(f"Changed paths: {len(changed)}", flush=True)
        selected = sorted(set(selected) & affected(metadata, changed, available))
    for field, actual in ((args.package, {t.package for t in available}), (args.target, {t.name for t in available})):
        if set(field) - actual:
            parser.error(f"unknown selection: {sorted(set(field) - actual)}")
    selected = [t for t in selected if (not args.package or t.package in args.package)
                and (not args.target or t.name in args.target)]
    if args.case and not args.target:
        parser.error("--case requires --target so unrelated suites are not run")
    if args.fast and changed and not selected:
        parser.error("no smoke targets cover this selection; omit --fast to run the affected tests")
    settings = {t: job_settings(t, args, smokes) for t in selected}
    if args.fast:
        print("Fast smoke plan; use just check for complete validation", flush=True)
    for t in selected:
        detail = ""
        if args.fast and (settings[t].filter or settings[t].case):
            detail = f" tests={settings[t].filter!r}, cases={settings[t].case!r}"
        print(f"{t.label} ({'exclusive' if t.exclusive else 'parallel'}){detail}")
    print(f"Selected {len(selected)} targets", flush=True)
    if not selected and (args.target or args.package or args.case):
        parser.error("selection contains no smoke targets; omit --fast for full targets" if args.fast else "selection contains no targets")
    if args.list or not selected:
        return 0
    started = time.monotonic()
    directory = ROOT / "out" / "test-runs" / f"{time.time_ns()}-{os.getpid()}"
    directory.mkdir(parents=True)
    print(f"Logs: {directory}", flush=True)
    (directory / "plan.json").write_text(json.dumps({
        "group": args.group, "fast": args.fast, "jobs": args.jobs, "case": args.case, "filter": args.filter,
        "selections": {t.label: {"filter": settings[t].filter, "case": settings[t].case} for t in selected},
        "targets": [t.label for t in selected],
    }, indent=2) + "\n")
    executables = build(args.cargo, selected, directory, complete=selected == available)
    deduplicate = owns_native_sweep(selected, args)
    records = []
    # One outer budget: ordinary binaries are single-threaded; large jobs run alone.
    normal = [t for t in selected if not t.exclusive]
    exclusive = [t for t in selected if t.exclusive]
    control = RunControl()
    handlers = {kind: signal.signal(kind, lambda *_: control.cancel())
                for kind in (signal.SIGINT, signal.SIGTERM)}
    try:
        with ThreadPoolExecutor(max_workers=args.jobs) as pool:
            futures = [pool.submit(run_target, t, executables[t], settings[t], directory, deduplicate, control) for t in normal]
            for future in as_completed(futures):
                records.append(future.result())
                (directory / "results.json").write_text(json.dumps(records, indent=2) + "\n")
        for target in exclusive:
            if control.stopped.is_set():
                break
            records.append(run_target(target, executables[target], settings[target], directory, deduplicate, control))
            (directory / "results.json").write_text(json.dumps(records, indent=2) + "\n")
    finally:
        for kind, handler in handlers.items():
            signal.signal(kind, handler)
    if control.stopped.is_set():
        print("Cancelled; active test process groups were stopped", file=sys.stderr)
        return 130
    if not args.fast and args.group == "all" and args.changed is None and not (args.package or args.target or args.filter or args.case):
        start = time.monotonic()
        with (directory / "doctests.log").open("w") as output:
            result = subprocess.run([args.cargo, "test", "--workspace", "--doc"], cwd=ROOT,
                                    stdout=output, stderr=subprocess.STDOUT)
        records.append({"target": "workspace::doctests", "exit_code": result.returncode,
                        "seconds": round(time.monotonic() - start, 2)})
    (directory / "results.json").write_text(json.dumps(records, indent=2) + "\n")
    if args.case and not any("flux-case: " in Path(r["log"]).read_text() for r in records):
        print("ERROR: case filter matched no cases", file=sys.stderr)
        return 1
    failed = [r for r in records if r["exit_code"]]
    (directory / "summary.json").write_text(json.dumps({
        "targets": len(records), "failed": len(failed), "fast": args.fast,
        "wall_seconds": round(time.monotonic() - started, 2),
        "target_seconds": round(sum(r["seconds"] for r in records), 2),
        "native_sweep_owns_comparisons": deduplicate,
    }, indent=2) + "\n")
    print(f"Finished {len(records)} targets; {len(failed)} failed. Results: {directory / 'results.json'}")
    return bool(failed)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (RuntimeError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
