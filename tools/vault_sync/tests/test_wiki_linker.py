from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from tools.vault_sync.config import LinkingConfig, VaultConfig
from tools.vault_sync.wiki_linker import NAV_START, link_vault


class WikiLinkerTests(unittest.TestCase):
    def test_linker_builds_hierarchy_topics_and_idempotent_navigation(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            vault = root / "vault"
            project = vault / "projects/alpha"
            project.mkdir(parents=True)
            (project / "plan.md").write_text("---\ntags: [launch, alpha]\n---\n# Launch Plan\n", encoding="utf-8")
            (project / "review.md").write_text("---\ntags: [launch]\n---\n# Launch Review\n", encoding="utf-8")
            config = LinkingConfig(
                enabled=True,
                default_project_prefix="projects",
                project_aliases={},
                min_tag_documents=2,
            )
            vault_config = VaultConfig(vault, root / "state")

            first = link_vault(vault_config, config)
            plan_after_first = (project / "plan.md").read_text(encoding="utf-8")
            second = link_vault(vault_config, config)
            plan_after_second = (project / "plan.md").read_text(encoding="utf-8")

            self.assertEqual(first.documents, 2)
            self.assertTrue((vault / "_memento.md").exists())
            self.assertTrue((project / "_memento_hub.md").exists())
            self.assertTrue((vault / "_memento/topics/launch.md").exists())
            self.assertIn("[[projects/alpha/plan|Launch Plan]]", (project / "_memento_hub.md").read_text())
            self.assertEqual(plan_after_first.count(NAV_START), 1)
            self.assertEqual(plan_after_first, plan_after_second)
            self.assertEqual(second.navigation_updated, 0)

    def test_linker_refuses_to_overwrite_user_owned_hub(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            vault = root / "vault"
            vault.mkdir()
            (vault / "note.md").write_text("# Note\n", encoding="utf-8")
            (vault / "_memento.md").write_text("# My own hub\n", encoding="utf-8")
            config = LinkingConfig(enabled=True, default_project_prefix="projects", project_aliases={})

            result = link_vault(VaultConfig(vault, root / "state"), config)

            self.assertGreaterEqual(result.failed, 1)
            self.assertEqual((vault / "_memento.md").read_text(encoding="utf-8"), "# My own hub\n")

    def test_custom_hub_names_are_used_in_parent_and_topic_links(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            vault = root / "vault"
            nested = vault / "area/nested"
            nested.mkdir(parents=True)
            for name in ("one", "two"):
                (nested / f"{name}.md").write_text(
                    f"---\ntags: [shared]\n---\n# {name.title()}\n", encoding="utf-8"
                )
            config = LinkingConfig(
                enabled=True,
                default_project_prefix="projects",
                project_aliases={},
                root_hub="home.md",
                hub_filename="index.md",
            )

            link_vault(VaultConfig(vault, root / "state"), config)

            self.assertIn("[[home|← Parent hub]]", (vault / "area/index.md").read_text())
            self.assertIn("[[area/index|← Parent hub]]", (nested / "index.md").read_text())
            self.assertIn("[[home|← Memento]]", (vault / "_memento/topics/shared.md").read_text())
