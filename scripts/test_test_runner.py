"""Check planning independently of Cargo, compiler behavior, or machine timing."""
import argparse
import json
import subprocess
from types import SimpleNamespace
from pathlib import Path
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parent))
import test as runner


def metadata():
    packages = []
    for name, dependencies in (("flux-syntax", []), ("flux-hir", ["flux-syntax"]),
                               ("flux-ty-ir", ["flux-hir"]), ("flux-solve", ["flux-ty-ir"]),
                               ("flux-ty", ["flux-hir", "flux-ty-ir", "flux-solve"]),
                               ("flux-mir", ["flux-ty", "flux-solve"]), ("flux-cli", ["flux-mir"])):
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

    def test_type_ir_and_solver_changes_select_the_type_checker(self):
        selected = self.affected("crates/flux-ty-ir/src/ty.rs")
        self.assertEqual({t.package for t in selected},
                         {"flux-ty-ir", "flux-solve", "flux-ty", "flux-mir", "flux-cli"})
        selected = self.affected("crates/flux-solve/src/traits.rs")
        self.assertEqual({t.package for t in selected}, {"flux-solve", "flux-ty", "flux-mir", "flux-cli"})

    def test_type_ir_and_solver_belong_to_inference(self):
        for package in ("flux-ty-ir", "flux-solve", "flux-ty"):
            target = runner.Target(package, package.replace("-", "_"), "lib", f"/{package}/src/lib.rs")
            self.assertTrue(runner.in_group(target, "inference"), package)
            self.assertFalse(runner.in_group(target, "borrowing"), package)

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

    def test_only_timing_sensitive_targets_run_exclusively(self):
        def target(package, name):
            return runner.Target(package, name, "test", f"/{package}/tests/{name}.rs")
        for package, name in (("flux-mir", "scaling"), ("flux-ty", "solver_scaling"), ("flux-ty", "trait_solver")):
            self.assertTrue(target(package, name).exclusive, name)
        for package, name in (("flux-cli", "run"), ("flux-cli", "mir_selfhost"), ("flux-cli", "proof_compatibility"),
                              ("flux-cli", "keel_asm_readiness"), ("flux-cli", "native_fixtures"),
                              ("flux-cli", "trait_solver")):
            self.assertFalse(target(package, name).exclusive, name)

    def test_reduced_keel_suite_is_a_source_graph_in_the_pool(self):
        target = runner.Target("flux-cli", "keel_reduced_suite", "test",
                               "/flux-cli/tests/keel_reduced_suite.rs")
        self.assertTrue(runner.in_group(target, "graphs"))
        self.assertFalse(target.exclusive)
        self.assertFalse(runner.in_group(target, "codegen"))

    def test_default_jobs_leave_headroom_and_follow_the_environment(self):
        self.assertEqual(runner.default_jobs({}, cpus=18), 14)
        self.assertEqual(runner.default_jobs({}, cpus=4), 3)
        self.assertEqual(runner.default_jobs({}, cpus=1), 1)
        self.assertEqual(runner.default_jobs({"FLUX_TEST_JOBS": "6"}, cpus=18), 6)
        for value in ("0", "-1", "many"):
            with self.assertRaisesRegex(RuntimeError, "FLUX_TEST_JOBS"):
                runner.default_jobs({"FLUX_TEST_JOBS": value}, cpus=18)

    def test_internally_parallel_targets_hold_part_of_the_budget(self):
        run = runner.Target("flux-cli", "run", "test", "/flux-cli/tests/run.rs")
        proof = runner.Target("flux-cli", "proof_compatibility", "test", "/flux-cli/tests/proof_compatibility.rs")
        other = runner.Target("flux-mir", "run", "test", "/flux-mir/tests/run.rs")
        self.assertEqual(runner.workers(run, 14, alone=False), 7)
        self.assertEqual(runner.workers(run, 14, alone=True), 14)
        self.assertEqual(runner.workers(run, 1, alone=False), 1)
        self.assertEqual(runner.workers(proof, 14, alone=False), 4)
        self.assertEqual(runner.workers(proof, 2, alone=False), 2)
        self.assertEqual(runner.workers(other, 14, alone=False), 1)

    def write_run(self, directory, name, records):
        run = directory / name
        run.mkdir(parents=True)
        (run / "results.json").write_text(json.dumps(records))
        return run

    def test_history_takes_the_newest_passing_unfiltered_record(self):
        def record(target, seconds, **extra):
            return {"target": target, "seconds": seconds, "exit_code": 0, "filter": None, "case": None, **extra}
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.write_run(directory, "100-1", [record("a", 50), record("b", 9), record("c", 7)])
            self.write_run(directory, "200-1", [record("a", 5, filter="fixtures"), record("b", 40, exit_code=1),
                                                record("c", 3)])
            (directory / "300-1").mkdir()  # Interrupted before any result.
            (directory / "notes").mkdir()
            runs = runner.previous_runs(directory)
            self.assertEqual([r.name for r in runs], ["300-1", "200-1", "100-1"])
            previous = runner.history(runs)
            self.assertEqual({k: v["seconds"] for k, v in previous.items()}, {"a": 50, "b": 9, "c": 3})
        self.assertEqual(runner.previous_runs(Path("/nonexistent/test-runs")), [])

    def test_pool_runs_longest_first_and_unknown_targets_lead(self):
        names = ["alpha", "beta", "gamma", "delta", "epsilon"]
        pool = [runner.Target("p", name, "test", f"/p/tests/{name}.rs") for name in names]
        previous = {"p::alpha": {"seconds": 1}, "p::beta": {"seconds": 60},
                    "p::gamma": {"seconds": 30}, "p::epsilon": {"seconds": 40, "workers": 8}}
        settings = {t: argparse.Namespace(workers=2 if t.name == "epsilon" else 1) for t in pool}
        order = runner.run_order(pool, previous, settings)
        # epsilon had 8 workers and now has 2: it should take four times as long.
        self.assertEqual([t.name for t in order], ["delta", "epsilon", "beta", "gamma", "alpha"])
        self.assertEqual(runner.run_order(pool, {}, settings), pool)

    def test_deadline_scales_previous_duration_with_a_floor(self):
        self.assertEqual(runner.deadline(None, 1), runner.TIMEOUT_UNKNOWN)
        self.assertEqual(runner.deadline({"seconds": 2}, 1), runner.TIMEOUT_FLOOR)
        self.assertEqual(runner.deadline({"seconds": 100}, 1), 100 * runner.TIMEOUT_FACTOR)
        self.assertEqual(runner.deadline({"seconds": 100, "workers": 4}, 2), 200 * runner.TIMEOUT_FACTOR)
        self.assertEqual(runner.deadline({"seconds": 100, "workers": 2}, 4), 100 * runner.TIMEOUT_FACTOR)

    def test_prune_keeps_the_newest_runs_only(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for name in ("100-1", "900-2", "1000-3", "50-9"):
                self.write_run(directory, name, [])
            (directory / "keep-me").mkdir()
            runner.prune(directory, 2)
            self.assertEqual(sorted(p.name for p in directory.iterdir()), ["1000-3", "900-2", "keep-me"])

    def test_schedule_respects_weights_and_fills_free_slots(self):
        lock = threading.Lock()
        started, active, peak = [], [0], [0]
        weights = {"a": 1, "wide": 3, "b": 1, "c": 1}

        def run(item):
            with lock:
                started.append(item)
                active[0] += weights[item]
                peak[0] = max(peak[0], active[0])
            time.sleep(0.05)
            with lock:
                active[0] -= weights[item]
            return item

        finished = []
        runner.schedule(3, ["a", "wide", "b", "c"], weights.get, run, finished.append, threading.Event())
        self.assertEqual(started[:2], ["a", "b"], "b fills the slot that wide cannot use yet")
        self.assertEqual(sorted(finished), ["a", "b", "c", "wide"])
        self.assertEqual(peak[0], 3)

    def test_schedule_reraises_job_errors_after_running_jobs_finish(self):
        def run(item):
            if item == "bad":
                raise ValueError("broken job")
            time.sleep(0.05)
            return item

        finished = []
        with self.assertRaisesRegex(ValueError, "broken job"):
            runner.schedule(2, ["good", "bad"], lambda _: 1, run, finished.append, threading.Event())
        self.assertEqual(finished, ["good"])

    def test_cancelled_schedule_starts_nothing(self):
        stopped = threading.Event()
        stopped.set()
        runner.schedule(2, ["a"], lambda _: 1, lambda item: self.fail("started"), lambda _: None, stopped)

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

    def fake_libtest(self, directory, listed):
        """An executable that lists `listed` tests and, when run, prints one case."""
        executable = directory / "fake"
        lines = "".join(f"{name}: test\\n" for name in listed)
        executable.write_text(f"#!/bin/sh\nif [ \"$1\" = --list ]; then printf '{lines}'; exit 0; fi\n"
                              "printf 'running 1 test\\nflux-case: existing\\n'\n")
        executable.chmod(0o755)
        return executable

    def test_empty_smoke_function_and_case_filters_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            target = runner.Target("fixture", "empty", "test", str(directory / "tests" / "empty.rs"))
            args = argparse.Namespace(fast=True, jobs=1, case=None, filter="renamed", timeout=5)
            executable = self.fake_libtest(directory, [])
            result = runner.run_target(target, executable, args, directory, False)
            self.assertEqual(result["exit_code"], 1)
            self.assertIn("selected no test functions", Path(result["log"]).read_text())
            executable = self.fake_libtest(directory, ["fixtures"])
            args.case = "existing,missing"
            self.assertEqual(runner.run_target(target, executable, args, directory, False)["exit_code"], 1)
            args.case = "existing"
            self.assertEqual(runner.run_target(target, executable, args, directory, False)["exit_code"], 0)

    def test_smoke_filter_may_select_any_nonzero_number_of_tests(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            target = runner.Target("fixture", "partial", "test", str(directory / "tests" / "partial.rs"))
            args = argparse.Namespace(fast=True, jobs=1, case=None, filter="fixtures", timeout=5)
            for listed in (["fixtures_ok"], ["fixtures_ok", "fixtures_err", "fixtures_new"]):
                executable = self.fake_libtest(directory, listed)
                self.assertEqual(runner.run_target(target, executable, args, directory, False)["exit_code"], 0)

    def test_multi_package_plan_builds_with_one_command(self):
        first = runner.Target("first", "a", "test", "/first/tests/a.rs", "first-id")
        lib = runner.Target("first", "first", "lib", "/first/src/lib.rs", "first-id")
        second = runner.Target("second", "second", "lib", "/second/src/lib.rs", "second-id")
        binary = runner.Target("second", "tool", "bin", "/second/src/main.rs", "second-id")
        selected = [first, lib, second, binary, runner.RUNNER]
        messages = [{"reason": "compiler-artifact", "package_id": t.package_id, "profile": {"test": True},
                     "target": {"name": t.name, "kind": [t.kind], "src_path": t.source},
                     "executable": f"/target/{t.name}"} for t in selected if t.kind != "python"]
        result = SimpleNamespace(stdout="\n".join(json.dumps(m) for m in messages), returncode=0)
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(runner.subprocess, "run", return_value=result) as run:
            executables = runner.build("cargo", selected, Path(temporary))
        self.assertEqual(run.call_count, 1)
        self.assertEqual(run.call_args[0][0], ["cargo", "test", "--no-run", "--message-format=json",
                                               "-p", "first", "-p", "second",
                                               "--test", "a", "--lib", "--bin", "tool"])
        self.assertEqual(executables[binary], Path("/target/tool"))
        self.assertEqual(executables[runner.RUNNER], Path(sys.executable))

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
            self.assertEqual(result["workers"], 1)

    def test_runtime_sweep_gets_its_share_of_the_budget(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            executable = directory / "run"
            executable.write_text('#!/bin/sh\ntest "$FLUX_TEST_JOBS" = "7"\n')
            executable.chmod(0o755)
            target = runner.Target("flux-cli", "run", "test", str(directory / "tests" / "run.rs"))
            args = argparse.Namespace(jobs=14, workers=7, case=None, filter=None, timeout=5)
            result = runner.run_target(target, executable, args, directory, False)
            self.assertEqual(result["exit_code"], 0)
            self.assertEqual(result["workers"], 7)


if __name__ == "__main__":
    unittest.main()
