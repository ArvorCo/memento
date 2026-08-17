from __future__ import annotations

import sqlite3
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.vault_sync.config import DatabaseImportConfig, VaultConfig
from tools.vault_sync.database_import import DatabaseImportError, import_database


class DatabaseImportTests(unittest.TestCase):
    def test_sqlite_rows_import_incrementally_and_delete_removed_documents(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            database = root / "notes.db"
            with sqlite3.connect(database) as connection:
                connection.execute(
                    "CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT, body TEXT, updated_at TEXT)"
                )
                connection.execute("INSERT INTO notes VALUES (1, 'Alpha', 'First body', '2026-01-01')")
            config = self._config(root, database)
            vault = VaultConfig(root / "vault", root / "state")

            first = import_database(vault, config)
            second = import_database(vault, config)
            with sqlite3.connect(database) as connection:
                connection.execute("UPDATE notes SET body = 'Changed body' WHERE id = 1")
            third = import_database(vault, config)

            outputs = list((vault.root / "database/notes").glob("*.md"))
            self.assertEqual(first.imported, 1)
            self.assertEqual(second.skipped, 1)
            self.assertEqual(third.updated, 1)
            self.assertEqual(len(outputs), 1)
            self.assertIn("Changed body", outputs[0].read_text(encoding="utf-8"))
            self.assertNotIn(str(database), outputs[0].read_text(encoding="utf-8"))

            with sqlite3.connect(database) as connection:
                connection.execute("DELETE FROM notes")
            fourth = import_database(vault, config)
            self.assertEqual(fourth.removed, 1)
            self.assertEqual(list((vault.root / "database/notes").glob("*.md")), [])

    def test_mutating_query_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            database = root / "notes.db"
            database.touch()
            config = self._config(root, database)
            config.query = "DELETE FROM notes"

            with self.assertRaises(DatabaseImportError):
                import_database(VaultConfig(root / "vault", root / "state"), config)

    def test_remote_import_opens_read_only_transaction_and_rolls_back(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            config = self._config(root, root / "unused.db")
            config.driver = "postgres"
            config.database = None
            config.dsn_env = "MEMENTO_TEST_DSN"
            cursor = mock.MagicMock()
            cursor.description = [("id",), ("title",), ("body",), ("updated_at",)]
            cursor.fetchmany.side_effect = [[(1, "Alpha", "Body", "2026-01-01")], []]
            connection = mock.MagicMock()
            connection.cursor.return_value = cursor
            module = mock.MagicMock()
            module.connect.return_value = connection

            with (
                mock.patch.dict("os.environ", {"MEMENTO_TEST_DSN": "postgres://example"}),
                mock.patch("tools.vault_sync.database_import.importlib.import_module", return_value=module),
            ):
                result = import_database(VaultConfig(root / "vault", root / "state"), config)

            self.assertEqual(result.imported, 1)
            self.assertEqual(cursor.execute.call_args_list[0], mock.call("BEGIN READ ONLY"))
            self.assertEqual(cursor.execute.call_args_list[1], mock.call(config.query))
            connection.rollback.assert_called_once_with()

    def test_mysql_url_is_parsed_without_exposing_credentials(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            config = self._config(root, root / "unused.db")
            config.driver = "mysql"
            config.database = None
            config.dsn_env = "MEMENTO_TEST_MYSQL_DSN"
            cursor = mock.MagicMock()
            cursor.description = [("id",), ("title",), ("body",), ("updated_at",)]
            cursor.fetchmany.side_effect = [[], []]
            connection = mock.MagicMock()
            connection.cursor.return_value = cursor
            module = mock.MagicMock()
            module.connect.return_value = connection

            with (
                mock.patch.dict(
                    "os.environ",
                    {"MEMENTO_TEST_MYSQL_DSN": "mysql://reader:s%40fe@db.example:3307/brain?charset=utf8mb4"},
                ),
                mock.patch("tools.vault_sync.database_import.importlib.import_module", return_value=module),
            ):
                result = import_database(VaultConfig(root / "vault", root / "state"), config)

            self.assertEqual(result.rows_read, 0)
            module.connect.assert_called_once_with(
                host="db.example",
                port=3307,
                user="reader",
                password="s@fe",
                database="brain",
                charset="utf8mb4",
            )
            self.assertEqual(cursor.execute.call_args_list[0], mock.call("START TRANSACTION READ ONLY"))

    @staticmethod
    def _config(root: Path, database: Path) -> DatabaseImportConfig:
        return DatabaseImportConfig(
            name="notes",
            enabled=True,
            driver="sqlite",
            database=str(database),
            dsn_env=None,
            query="SELECT id, title, body, updated_at FROM notes ORDER BY id",
            destination=Path("database/notes"),
            manifest=root / "state/notes.json",
            id_column="id",
            title_column="title",
            content_columns=["body"],
            metadata_columns=["updated_at"],
            updated_at_column="updated_at",
            tags=["database", "notes"],
            delete_removed=True,
        )
