import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = REPO_ROOT / "scripts" / "real_world_fcs_suite.py"
MODULE_SPEC = importlib.util.spec_from_file_location(
    "real_world_fcs_suite", MODULE_PATH
)
assert MODULE_SPEC is not None
assert MODULE_SPEC.loader is not None
real_world_fcs_suite = importlib.util.module_from_spec(MODULE_SPEC)
MODULE_SPEC.loader.exec_module(real_world_fcs_suite)


class RunCliTests(unittest.TestCase):
    def test_falls_back_to_cargo_when_binary_is_stale(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cli_path = Path(temp_dir) / "flowjoish-cli"
            cli_path.write_text("", encoding="utf-8")
            dataset_path = Path(temp_dir) / "sample.fcs"
            dataset_path.write_text("", encoding="utf-8")

            stale_binary = subprocess.CompletedProcess(
                args=[str(cli_path), "inspect-fcs-json", str(dataset_path)],
                returncode=1,
                stdout="",
                stderr="usage: flowjoish-cli <inspect-fcs|demo-replay> [args]\n",
            )
            cargo_fallback = subprocess.CompletedProcess(
                args=[
                    "cargo",
                    "run",
                    "-q",
                    "-p",
                    "flowjoish-cli",
                    "--",
                    "inspect-fcs-json",
                    str(dataset_path),
                ],
                returncode=0,
                stdout='{"status":"ok"}\n',
                stderr="",
            )

            with mock.patch.object(
                real_world_fcs_suite.subprocess,
                "run",
                side_effect=[stale_binary, cargo_fallback],
            ) as run_mock:
                result = real_world_fcs_suite.run_cli(cli_path, dataset_path)

        self.assertEqual(result, cargo_fallback)
        self.assertEqual(run_mock.call_count, 2)
        first_call = run_mock.call_args_list[0]
        second_call = run_mock.call_args_list[1]
        self.assertEqual(
            first_call.args[0],
            [str(cli_path), "inspect-fcs-json", str(dataset_path)],
        )
        self.assertEqual(
            second_call.args[0],
            [
                "cargo",
                "run",
                "-q",
                "-p",
                "flowjoish-cli",
                "--",
                "inspect-fcs-json",
                str(dataset_path),
            ],
        )

    def test_does_not_fallback_on_real_parser_error(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cli_path = Path(temp_dir) / "flowjoish-cli"
            cli_path.write_text("", encoding="utf-8")
            dataset_path = Path(temp_dir) / "sample.fcs"
            dataset_path.write_text("", encoding="utf-8")

            parser_error = subprocess.CompletedProcess(
                args=[str(cli_path), "inspect-fcs-json", str(dataset_path)],
                returncode=1,
                stdout="",
                stderr="failed to parse FCS: TEXT segment did not terminate cleanly\n",
            )

            with mock.patch.object(
                real_world_fcs_suite.subprocess,
                "run",
                return_value=parser_error,
            ) as run_mock:
                result = real_world_fcs_suite.run_cli(cli_path, dataset_path)

        self.assertEqual(result, parser_error)
        self.assertEqual(run_mock.call_count, 1)


class FallbackPredicateTests(unittest.TestCase):
    def test_usage_without_new_subcommand_triggers_fallback(self) -> None:
        stale_binary = subprocess.CompletedProcess(
            args=["flowjoish-cli"],
            returncode=1,
            stdout="",
            stderr="usage: flowjoish-cli <inspect-fcs|demo-replay> [args]\n",
        )

        self.assertTrue(real_world_fcs_suite.should_fallback_to_cargo(stale_binary))

    def test_usage_with_new_subcommand_does_not_trigger_fallback(self) -> None:
        current_binary = subprocess.CompletedProcess(
            args=["flowjoish-cli"],
            returncode=1,
            stdout="",
            stderr="usage: flowjoish-cli <inspect-fcs|inspect-fcs-json|demo-replay> [args]\n",
        )

        self.assertFalse(real_world_fcs_suite.should_fallback_to_cargo(current_binary))


if __name__ == "__main__":
    unittest.main()
