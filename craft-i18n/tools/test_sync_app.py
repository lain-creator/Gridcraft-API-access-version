"""Tests for sync_app.py on small synthetic apps: python3 -m unittest discover tools"""

import contextlib
import io
import subprocess
import tempfile
import unittest
from pathlib import Path

import sync_app

DIR = "crates/ui-egui/src/i18n"


def header(name, locale, app):
    return sync_app.HEADER.format(name=name, locale=locale, app=app)


def tsv(*rows):
    return "".join("\t".join(r) + "\n" for r in rows)


class Sync(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name, "craft-i18n")
        self.app = Path(tmp.name, "demo")
        self.app.mkdir()
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "t@example.com")
        self.git("config", "user.name", "t")
        self.locale("fr", "Français", common=[("", "Save", "Enregistrer"), ("", "Open", "Ouvrir")],
                    app=[("", "Crop", "Rogner")])
        self.locale("es", "Español", common=[("", "Save", "Guardar")], app=[("", "Crop", "Recortar")])
        self.write_app({"fr.tsv": tsv(("", "Save", "Enregistrer"), ("", "Crop", "Rogner")),
                        "es.tsv": tsv(("", "Save", "Guardar"), ("", "Crop", "Recortar")),
                        "mod.rs": 'LangInfo { code: "fr", name: "Français" },\n'
                                  'LangInfo { code: "es-MX", name: "Español (México)" },\n'})
        self.base = self.commit()

    def git(self, *args):
        return subprocess.run(["git", "-C", self.app, *args], check=True, capture_output=True,
                              text=True).stdout.strip()

    def locale(self, locale, name, common=(), app=None):
        d = self.root / "locales" / locale
        d.mkdir(parents=True)
        (d / "common.tsv").write_text(
            header(name, locale, "every Crafting App") + "\n" + tsv(("@id", "language.name", name), *common))
        if app is not None:
            (d / "demo.tsv").write_text(header(name, locale, "Demo") + "\n" + tsv(*app))

    def write_app(self, files):
        d = self.app / DIR
        d.mkdir(parents=True, exist_ok=True)
        for name, text in files.items():
            if text is None:
                (d / name).unlink()
            else:
                (d / name).write_text(text)

    def commit(self):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "x", "--allow-empty")
        return self.git("rev-parse", "HEAD")

    def run_sync(self, *args, base=None):
        out = io.StringIO()
        argv = [str(self.app), base or self.base, "--root", str(self.root), "--app", "demo", *args]
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(out):
            try:
                code = sync_app.main(argv)
            except SystemExit as e:
                code = e.code if isinstance(e.code, int) else 1
                print(e.code)
        return code, out.getvalue()

    def rows(self, locale, file="demo.tsv"):
        path = self.root / "locales" / locale / file
        return sync_app.parse(path.read_text())[0] if path.exists() else None

    def snapshot(self):
        return {p: p.read_bytes() for p in sorted(self.root.rglob("*.tsv"))}

    def test_unchanged_app_changes_nothing_and_round_trips(self):
        before = self.snapshot()
        code, out = self.run_sync()
        self.assertEqual(code, 0, out)
        self.assertEqual(self.snapshot(), before)
        self.assertIn(self.base, out)
        self.assertEqual(self.run_sync("--check")[0], 0)

    def test_new_and_corrected_app_strings_go_to_the_app_catalog(self):
        self.write_app({"fr.tsv": tsv(("", "Save", "Enregistrer"), ("", "Crop", "Recadrer"),
                                      ("", "Blur", "Flou"))})
        self.commit()
        self.assertEqual(self.run_sync()[0], 0)
        self.assertEqual(self.rows("fr"), {("", "Blur"): "Flou", ("", "Crop"): "Recadrer"})
        self.assertEqual(self.run_sync("--check")[0], 0)

    def test_changed_shared_string_becomes_an_override_and_common_never_changes(self):
        common = (self.root / "locales/fr/common.tsv").read_bytes()
        self.write_app({"fr.tsv": tsv(("", "Save", "Sauvegarder"), ("", "Crop", "Rogner"))})
        synced = self.commit()
        self.run_sync()
        self.assertEqual(self.rows("fr")[("", "Save")], "Sauvegarder")
        self.assertEqual((self.root / "locales/fr/common.tsv").read_bytes(), common)
        # Back to the shared wording: the override would change nothing, so it goes.
        self.write_app({"fr.tsv": tsv(("", "Save", "Enregistrer"), ("", "Crop", "Rogner"))})
        self.commit()
        self.assertEqual(self.run_sync(base=synced)[0], 0)
        self.assertNotIn(("", "Save"), self.rows("fr"))

    def test_deleted_rows_leave_the_app_catalog_and_an_unused_shared_row_is_reported(self):
        self.write_app({"fr.tsv": tsv(("", "Save", "Enregistrer"), ("", "Crop", "Rogner"), ("", "Open", "Ouvrir ici"))})
        base = self.commit()
        self.run_sync()
        self.assertEqual(self.rows("fr")[("", "Open")], "Ouvrir ici")
        self.write_app({"fr.tsv": tsv(("", "Save", "Enregistrer"), ("", "Crop", "Rogner"))})
        self.commit()
        code, out = self.run_sync(base=base)
        self.assertEqual(code, 0, out)
        self.assertNotIn(("", "Open"), self.rows("fr"))
        self.assertIn("shared row no longer used by demo", out)
        self.assertIn("Open", out)
        # A row the app gave up everywhere also leaves locales where it kept English.
        self.locale("de", "Deutsch", common=[("", "Save", "Speichern")], app=[("", "Save", "Save")])
        base = self.commit()
        self.write_app({"fr.tsv": tsv(("", "Crop", "Rogner")), "es.tsv": tsv(("", "Crop", "Recortar"))})
        self.commit()
        self.run_sync(base=base)
        self.assertEqual(self.rows("de"), {})
        self.assertIn("Speichern", (self.root / "locales/de/common.tsv").read_text())

    def test_new_locale_is_lowercase_bcp47_with_its_language_name(self):
        self.write_app({"pt_BR.tsv": tsv(("", "Crop", "Cortar")), "zh-TW.tsv": tsv(("", "Crop", "裁切")),
                        "ka.tsv": tsv(("", "Crop", "ამოჭრა"))})
        self.commit()
        code, out = self.run_sync()
        self.assertEqual(code, 0, out)
        self.assertEqual(self.rows("pt-br"), {("", "Crop"): "Cortar"})
        self.assertEqual(self.rows("zh-hant"), {("", "Crop"): "裁切"})
        self.assertIn("ka: no language name", out)
        self.write_app({"es-MX.tsv": tsv(("", "Crop", "Cortar"))})
        self.commit()
        self.run_sync()
        self.assertEqual(self.rows("es-mx", "common.tsv"), {("@id", "language.name"): "Español (México)"})
        text = (self.root / "locales/es-mx/demo.tsv").read_text()
        self.assertTrue(text.startswith("# Español (México) (es-mx) interface catalog for Demo.\n"))

    def test_regional_row_identical_to_its_base_language_is_not_written(self):
        self.write_app({"es-MX.tsv": tsv(("", "Save", "Guardar"), ("", "Crop", "Recortar"),
                                         ("", "Blur", "Desenfocar"), ("", "Open", "Abrir"))})
        self.commit()
        self.run_sync()
        self.assertEqual(self.rows("es-mx"), {("", "Blur"): "Desenfocar", ("", "Open"): "Abrir"})
        self.assertEqual(self.run_sync("--check")[0], 0)

    def test_msg_rows_never_reach_common(self):
        self.write_app({"es-MX.tsv": tsv(("@msg", "Couldn't open {name}", "No se pudo abrir {name}"))})
        self.commit()
        self.run_sync()
        self.assertEqual(self.rows("es-mx", "common.tsv"), {("@id", "language.name"): "Español (México)"})
        self.assertIn(("@msg", "Couldn't open {name}"), self.rows("es-mx"))

    def test_raw_escapes_survive_and_unknown_escapes_show_as_written(self):
        self.write_app({"fr.tsv": tsv(("", "Save", "Enregistrer"), ("", "Crop", "Rogner"),
                                      ("", "A\\nB", "C\\nD\\\\"), ("", "Use \\P", "Utiliser \\P"))})
        self.commit()
        self.run_sync()
        rows = self.rows("fr")
        self.assertEqual(rows[("", "A\\nB")], "C\\nD\\\\")
        self.assertEqual(rows[("", "Use \\\\P")], "Utiliser \\\\P")
        self.assertEqual(self.run_sync("--check")[0], 0)

    def test_plural_rows_get_the_locales_form_count(self):
        with (self.root / "locales/fr/common.tsv").open("a") as f:
            f.write("@plural\t{n} page|{n} pages\t{n} page|{n} pages\n")
        self.write_app({"fr.tsv": tsv(("", "Save", "Enregistrer"), ("", "Crop", "Rogner"),
                                      ("@plural", "{n} commit", "{n} commit"))})
        self.commit()
        self.run_sync()
        self.assertEqual(self.rows("fr")[("@plural", "{n} commit|{n} commits")], "{n} commit|{n} commit")

    def test_invalid_app_rows_are_skipped_and_reported(self):
        self.write_app({"fr.tsv": tsv(("", "Save", "Enregistrer"), ("", "Crop", "Rogner"),
                                      ("", "{n} files", "{count} fichiers"))})
        self.commit()
        code, out = self.run_sync()
        self.assertEqual(code, 0, out)
        self.assertNotIn(("", "{n} files"), self.rows("fr"))
        self.assertIn("{n} files", out)

    def test_msg_templates_that_never_match_are_skipped(self):
        self.write_app({"fr.tsv": tsv(("@msg", "Saved {n} in {folder}{note}", "{n} enregistrés dans {folder}{note}"),
                                      ("@msg", "{a}", "{a}"), ("@msg", "Open {x", "Ouvrir {x"),
                                      ("@msg", "Saved {n} files", "{n} fichiers enregistrés"))})
        self.commit()
        code, out = self.run_sync()
        self.assertEqual(code, 0, out)
        self.assertEqual([k for k in self.rows("fr") if k[0] == "@msg"], [("@msg", "Saved {n} files")])
        self.assertIn("two placeholders in a row", out)

    def test_check_lists_mismatches(self):
        (self.root / "locales/fr/demo.tsv").write_text(header("Français", "fr", "Demo") + "\n")
        code, out = self.run_sync("--check")
        self.assertEqual(code, 1)
        self.assertIn("Crop", out)

    def test_rerun_changes_nothing(self):
        self.write_app({"fr.tsv": tsv(("", "Save", "Sauver"), ("", "Blur", "Flou")),
                        "es-MX.tsv": tsv(("", "Crop", "Cortar")), "hu.tsv": tsv(("", "Crop", "Vágás"))})
        self.commit()
        self.run_sync()
        before = self.snapshot()
        self.run_sync()
        self.assertEqual(self.snapshot(), before)

    def test_rows_the_app_did_not_touch_keep_craft_i18n_wording(self):
        # A branch older than the last sync: main's newer wording stays.
        (self.root / "locales/fr/demo.tsv").write_text(
            header("Français", "fr", "Demo") + "\n" + tsv(("", "Crop", "Recadrer")))
        self.write_app({"es.tsv": tsv(("", "Save", "Guardar"), ("", "Crop", "Cortar"))})
        self.commit()
        code, out = self.run_sync()
        self.assertEqual(code, 0, out)  # reported, not a round-trip failure
        self.assertIn("untouched, craft-i18n differs: fr: \tCrop", out)
        self.assertEqual(self.rows("fr"), {("", "Crop"): "Recadrer"})
        self.assertEqual(self.rows("es"), {("", "Crop"): "Cortar"})

    def test_app_keeps_english_where_a_shared_row_means_something_else(self):
        self.locale("de", "Deutsch", common=[("", "Open", "Offen")], app=[])
        self.locale("it", "Italiano", common=[("", "Open", "Aperto")], app=[])
        self.locale("pt", "Português", common=[("", "Open", "Open")], app=[])
        self.write_app({"de.tsv": tsv(("", "Crop", "Zuschneiden")), "it.tsv": tsv(("", "Open", "Apri")),
                        "pt.tsv": tsv(("", "Crop", "Recortar")),
                        "es.tsv": tsv(("", "Save", "Guardar"), ("", "Crop", "Recortar"), ("", "Open", "Abrir"))})
        self.commit()
        self.run_sync()
        self.assertEqual(self.rows("de")[("", "Open")], "Open")
        self.assertNotIn(("", "Open"), self.rows("pt"))  # shared already English

    def test_refuses_a_ref_without_catalogs(self):
        self.write_app({"fr.tsv": None, "es.tsv": None})
        self.commit()
        code, out = self.run_sync()
        self.assertNotEqual(code, 0)
        self.assertIn("i18n/*.tsv", out)

    def test_header_matches_the_catalog_format_doc(self):
        doc = (Path(__file__).parent.parent / "docs/catalog-format.md").read_text()
        block = doc.split("## Header")[1].split("```\n")[1]
        self.assertEqual(block, sync_app.HEADER.format(
            name="<Language>", locale="<locale>", app="<every Crafting App | AppName>"))


if __name__ == "__main__":
    unittest.main()
