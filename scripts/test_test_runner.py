"""Check planning independently of Cargo, compiler behavior, or machine timing."""
import argparse
import json
import subprocess
from types import SimpleNamespace
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parent))
import test as runner


def metadata():
    packages = []
    for name, dependencies in (("flux-syntax", []), ("flux-hir", ["flux-syntax"]),
                               ("flux-ty", ["flux-hir"]), ("flux-mir", ["flux-ty"]),
                               ("flux-cli", ["flux-mir"])):
        root = runner.ROOT / "crates" / name
        names = [name.removeprefix("flux-")]
        if name == "flux-cli":
            names += ["run", "mir_selfhost", "native_fixtures"]
        packages.append({"name": name, "id": name, "manifest_path": str(root / "Cargo.toml"),
                         "dependencies": [{"name": d} for d in dependencies],
                         "targets": [{"name": t, "kind": ["test"], "test": True,
                                      "src_path": str(root / "tests" / f"{t}.rs")} for t in names]})
    return {"workspace_members": [p["id"] for p in packages], "packages": packages}


class Planning(unittest.TestCase):
    def setUp(self):
        self.metadata = metadata()
        self.targets = runner.targets(self.metadata)

    def affected(self, *paths):
        return runner.affected(self.metadata, list(paths), self.targets)

    def test_source_change_selects_all_downstream_targets_including_graphs(self):
        selected = self.affected("crates/flux-ty/src/infer.rs")
        self.assertEqual({t.package for t in selected}, {"flux-ty", "flux-mir", "flux-cli"})
        self.assertTrue(any(t.name == "mir_selfhost" for t in selected))

    def test_library_unknown_and_configuration_changes_fall_back_to_all(self):
        for path in ("library/core/src/ptr.rs", "Cargo.lock", "scripts/new.py", "README.md"):
            self.assertEqual(self.affected(path), set(self.targets), path)

    def test_independent_test_file_selects_its_binary(self):
        selected = self.affected("crates/flux-mir/tests/mir.rs")
        self.assertEqual([(t.package, t.name) for t in selected], [("flux-mir", "mir")])

    def test_shared_fixtures_and_removed_targets_broaden_to_package(self):
        for path in ("crates/flux-mir/tests/data/err/a.rs", "crates/flux-mir/tests/removed.rs"):
            self.assertEqual(self.affected(path), {t for t in self.targets if t.package in {"flux-mir", "flux-cli"}})

    def test_runtime_edits_retain_specialized_cli_checks(self):
        self.assertEqual(self.affected("tests/run/c_strings.rs"),
                         {t for t in self.targets if t.package == "flux-cli"})

    def test_no_changes_selects_nothing(self):
        self.assertEqual(self.affected(), set())

    def test_groups_keep_graphs_separate_but_all_keeps_everything(self):
        self.assertEqual([t for t in self.targets if runner.in_group(t, "all")], self.targets)
        graphs = [t for t in self.targets if runner.in_group(t, "graphs")]
        self.assertEqual([t.name for t in graphs], ["mir_selfhost"])
        self.assertFalse(any(runner.in_group(t, "codegen") for t in graphs))

    def test_large_targets_run_exclusively(self):
        self.assertTrue(next(t for t in self.targets if t.name == "run").exclusive)
        self.assertTrue(next(t for t in self.targets if t.name == "mir_selfhost").exclusive)
        self.assertFalse(next(t for t in self.targets if t.name == "native_fixtures").exclusive)

    def smoke_targets(self):
        keys = {key for component in runner.FAST_COMPONENTS.values() for key in component}
        result = [runner.Target(package, name, "test", f"/{package}/tests/{name}.rs", package)
                  for package, name in sorted(keys)]
        result += [runner.Target("flux-mir", "flux_mir", "lib", "/flux-mir/src/lib.rs", "flux-mir")]
        result += [runner.Target("flux-cli", "proof_compatibility", "test", "/flux-cli/tests/proof_compatibility.rs"),
                   runner.Target("flux-cli", "mir_selfhost", "test", "/flux-cli/tests/mir_selfhost.rs"),
                   runner.Target("flux-mir", "scaling", "test", "/flux-mir/tests/scaling.rs")]
        return result

    def test_fast_plan_uses_only_units_and_explicit_smokes(self):
        plan = runner.fast_plan(self.smoke_targets(), "all", native=True)
        self.assertTrue(any(t.kind == "lib" for t in plan))
        self.assertFalse(any(t.name in {"mir_selfhost", "proof_compatibility", "scaling"} for t in plan))
        run = next(smoke for t, smoke in plan.items() if t.name == "run")
        self.assertEqual(run.filter, "programs_match_rustc")
        self.assertEqual(run.case, "aggregate_storage,const_function_pointers")

    def test_component_smoke_does_not_select_downstream_suites(self):
        plan = runner.fast_plan(self.smoke_targets(), "borrowing", native=True)
        self.assertEqual({t.package for t in plan}, {"flux-mir"})
        self.assertEqual({t.name for t in plan}, {"flux_mir", "mir"})

    def test_fast_plan_keeps_portable_library_checks_without_native_execution(self):
        plan = runner.fast_plan(self.smoke_targets(), "library", native=False)
        self.assertEqual({t.name for t in plan}, {"std", "test_support"})

    def test_graphs_cannot_be_silently_sampled(self):
        with self.assertRaisesRegex(RuntimeError, "no smoke mode"):
            runner.fast_plan(self.smoke_targets(), "graphs", native=True)

    def test_missing_smoke_target_fails_instead_of_reducing_coverage(self):
        available = [t for t in self.smoke_targets() if t.name != "targets"]
        with self.assertRaisesRegex(RuntimeError, "missing fast target"):
            runner.fast_plan(available, "codegen", native=True)

    def test_fast_runtime_sample_never_owns_other_native_comparisons(self):
        run = next(t for t in self.targets if t.name == "run")
        args = argparse.Namespace(fast=True, case=None, filter=None)
        self.assertFalse(runner.owns_native_sweep([run], args))
        args.fast = False
        self.assertTrue(runner.owns_native_sweep([run], args))
        args.case = "c_strings"
        self.assertFalse(runner.owns_native_sweep([run], args))

    def test_job_settings_keep_component_case_filters_without_mutating_global_args(self):
        plan = runner.fast_plan(self.smoke_targets(), "borrowing", native=True)
        target = next(t for t in plan if t.name == "mir")
        args = argparse.Namespace(fast=True, case=None, filter=None)
        job = runner.job_settings(target, args, plan)
        self.assertEqual(job.filter, "fixtures")
        self.assertEqual(job.case, "moves.rs,consts.rs")
        self.assertIsNone(args.case)
        args.case = "moves.rs"
        self.assertEqual(runner.job_settings(target, args, plan).case, "moves.rs")

    def test_empty_smoke_function_and_case_filters_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            executable = directory / "empty"
            target = runner.Target("fixture", "empty", "test", str(directory / "tests" / "empty.rs"))
            args = argparse.Namespace(fast=True, jobs=1, case=None, filter="renamed", timeout=5)
            executable.write_text("#!/bin/sh\nprintf 'running 0 tests\n'\n")
            executable.chmod(0o755)
            self.assertEqual(runner.run_target(target, executable, args, directory, False)["exit_code"], 1)
            executable.write_text("#!/bin/sh\nprintf 'running 1 test\nflux-case: existing\n'\n")
            args.case = "existing,missing"
            self.assertEqual(runner.run_target(target, executable, args, directory, False)["exit_code"], 1)
            args.case = "existing"
            self.assertEqual(runner.run_target(target, executable, args, directory, False)["exit_code"], 0)

    def test_smoke_cannot_silently_lose_one_of_its_fixture_functions(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            executable = directory / "partial"
            executable.write_text("#!/bin/sh\nprintf 'running 1 test\n'\n")
            executable.chmod(0o755)
            target = runner.Target("fixture", "partial", "test", str(directory / "tests" / "partial.rs"))
            args = argparse.Namespace(fast=True, jobs=1, case=None, filter="fixtures",
                                      expected_tests=2, timeout=5)
            self.assertEqual(runner.run_target(target, executable, args, directory, False)["exit_code"], 1)

    def test_workspace_artifacts_with_same_name_keep_their_package_identity(self):
        first = runner.Target("first", "shared", "test", "/first/tests/shared.rs", "first-id")
        second = runner.Target("second", "shared", "test", "/second/tests/shared.rs", "second-id")
        messages = [{"reason": "compiler-artifact", "package_id": t.package_id, "profile": {"test": True},
                     "target": {"name": "shared", "kind": ["test"], "src_path": t.source},
                     "executable": f"/target/{t.package}"} for t in (first, second)]
        result = SimpleNamespace(stdout="\n".join(json.dumps(m) for m in messages), returncode=0)
        with tempfile.TemporaryDirectory() as temporary, patch.object(runner.subprocess, "run", return_value=result):
            executables = runner.build("cargo", [first, second], Path(temporary), complete=True)
        self.assertEqual(executables[first], Path("/target/first"))
        self.assertEqual(executables[second], Path("/target/second"))

    def test_shared_source_file_still_has_distinct_package_artifacts(self):
        first = runner.Target("first", "shared", "test", "/shared/tests/shared.rs", "first-id")
        second = runner.Target("second", "shared", "test", "/shared/tests/shared.rs", "second-id")
        messages = [{"reason": "compiler-artifact", "package_id": t.package_id,
                     "profile": {"test": True},
                     "target": {"name": "shared", "kind": ["test"], "src_path": t.source},
                     "executable": f"/target/{t.package}"} for t in (first, second)]
        result = SimpleNamespace(stdout="\n".join(json.dumps(m) for m in messages), returncode=0)
        with tempfile.TemporaryDirectory() as temporary, patch.object(runner.subprocess, "run", return_value=result):
            executables = runner.build("cargo", [first, second], Path(temporary), complete=True)
        self.assertEqual(executables[first], Path("/target/first"))
        self.assertEqual(executables[second], Path("/target/second"))

    def test_cancellation_stops_active_children_and_rejects_new_jobs(self):
        control = runner.RunControl()
        child = control.start(["sh", "-c", "exec sleep 30"],
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.assertIsNotNone(child)
        try:
            control.cancel()
            self.assertNotEqual(child.wait(timeout=5), 0)
            self.assertIsNone(control.start(["sh", "-c", "exit 0"]))
        finally:
            child.kill()
            child.wait()
            control.finish(child)

    def test_cancellation_during_process_start_does_not_leave_a_child(self):
        control = runner.RunControl()
        popen = subprocess.Popen

        def start_then_cancel(*args, **kwargs):
            child = popen(*args, **kwargs)
            control.cancel()
            return child

        with patch.object(runner.subprocess, "Popen", side_effect=start_then_cancel):
            child = control.start(["sh", "-c", "exec sleep 30"])
        try:
            self.assertNotEqual(child.wait(timeout=5), 0)
        finally:
            child.kill()
            child.wait()
            control.finish(child)

    def test_timeout_is_reported_and_child_is_reaped(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            executable = directory / "slow"
            executable.write_text("#!/bin/sh\nexec sleep 30\n")
            executable.chmod(0o755)
            target = runner.Target("fixture", "timeout", "test", str(directory / "tests" / "slow.rs"))
            args = argparse.Namespace(jobs=1, case=None, filter=None, timeout=1)
            result = runner.run_target(target, executable, args, directory, False)
            self.assertTrue(result["timed_out"])
            self.assertNotEqual(result["exit_code"], 0)

    def test_successful_child_gets_explicit_worker_budget(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            executable = directory / "fast"
            executable.write_text('#!/bin/sh\ntest "$FLUX_TEST_JOBS" = "1"\n')
            executable.chmod(0o755)
            target = runner.Target("fixture", "fast", "test", str(directory / "tests" / "fast.rs"))
            args = argparse.Namespace(jobs=4, case=None, filter=None, timeout=5)
            with patch.dict("os.environ", {"FLUX_TEST_CASE": "stale", "FLUX_TEST_NATIVE_SWEEP": "1"}):
                result = runner.run_target(target, executable, args, directory, False)
            self.assertEqual(result["exit_code"], 0)
            self.assertEqual(result["source"], target.source)
            self.assertEqual(result["executable"], str(executable))


if __name__ == "__main__":
    unittest.main()
