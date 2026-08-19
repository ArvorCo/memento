from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
FIXTURES = Path(__file__).resolve().parent / "fixtures"


class VaultSyncCliTests(unittest.TestCase):
    def test_init_config_writes_platform_preset(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            output = Path(tempdir) / "memento-vault-sync.toml"
            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "tools.vault_sync.cli",
                    "init-config",
                    "--preset",
                    "linux",
                    "--output",
                    str(output),
                ],
                capture_output=True,
                text=True,
                check=True,
            )
            self.assertIn("preset=linux", result.stdout)
            text = output.read_text(encoding="utf-8")
            self.assertIn('source = "${HOME}/Projects"', text)
            self.assertIn("[session_import.chatgpt]", text)

    def test_run_all_executes_markdown_sync_and_imports(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            vault = root / "vault"
            state = root / "state"
            workspace = root / "workspace"
            sessions = root / "sessions"

            (workspace / "docs").mkdir(parents=True)
            (workspace / "docs" / "note.md").write_text("# Note\n", encoding="utf-8")
            (sessions / "codex").mkdir(parents=True)
            (sessions / "codex" / "sample.jsonl").write_text(
                (FIXTURES / "codex_session.jsonl").read_text(encoding="utf-8"),
                encoding="utf-8",
            )

            config = root / "config.toml"
            vault_value = json.dumps(str(vault))
            state_value = json.dumps(str(state))
            workspace_value = json.dumps(str(workspace))
            sessions_value = json.dumps(str(sessions / "codex"))
            config.write_text(
                textwrap.dedent(
                    f"""
                    [vault]
                    root = {vault_value}
                    state_dir = {state_value}

                    [[markdown_sync.roots]]
                    name = "workspace"
                    source = {workspace_value}
                    destination = "projects"
                    include_extensions = [".md"]
                    exclude_dirs = [".git"]
                    protected_globs = ["**/_*_hub.md"]

                    [linking]
                    enabled = true
                    default_project_prefix = "projects"

                    [linking.project_aliases]
                    memento = "projects/Memento/Memento"

                    [session_import.codex]
                    enabled = true
                    source = {sessions_value}
                    destination = "converted/codex"
                    manifest = "codex_manifest.json"
                    label = "Codex"
                    source_tag = "codex"
                    file_glob = "*.jsonl"
                    """
                ),
                encoding="utf-8",
            )

            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "tools.vault_sync.cli",
                    "--config",
                    str(config),
                    "run-all",
                ],
                capture_output=True,
                text=True,
                check=True,
            )

            self.assertIn("[workspace] copied=1 deleted=0 skipped=0 failed=0", result.stdout)
            self.assertIn("[codex] imported=1 skipped=0", result.stdout)
            self.assertTrue((vault / "projects" / "docs" / "note.md").exists())
            self.assertTrue(any((vault / "converted/codex").rglob("sample.md")))

            machine_result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "tools.vault_sync.cli",
                    "--config",
                    str(config),
                    "--json",
                    "run-all",
                ],
                capture_output=True,
                text=True,
                check=True,
            )
            payload = json.loads(machine_result.stdout)
            self.assertEqual(payload["status"], 0)
            self.assertEqual(payload["steps"][0]["name"], "sync-markdown")
            self.assertIn("skipped=1", payload["steps"][0]["stdout"][0])


if __name__ == "__main__":
    unittest.main()
