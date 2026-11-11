"""Regression tests for the standalone Slint architecture checker."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from check_ui_quality import LIMITS, ROOT, check


class QualityTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "ui").mkdir()

    def source(self, name, body):
        path = self.root / "ui" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")
        return path

    def rules(self, **limits):
        return {issue["rule"] for issue in check(self.root, limits)["issues"]}

    def test_comments_and_strings_do_not_create_fake_components_or_imports(self):
        self.source("main.slint", '''// component Fake { {
/* import { X } from "missing.slint"; } */
export component Main inherits Rectangle {
    Text { text: "component Fake { // text and \\"quoted\\" }"; }
}''')
        report = check(self.root)
        self.assertEqual(report["issues"], [])
        self.assertEqual([c["name"] for c in report["files"][0]["components"]], ["Main"])
        self.assertEqual(report["files"][0]["nesting"], 2)

    def test_size_and_nesting_report_source_locations(self):
        self.source("deep.slint", '''// comment
export component Main inherits Rectangle {
    Rectangle {
        Text { text: "large"; }
    }
}''')
        report = check(self.root, {"file_lines": 4, "component_lines": 4, "nesting": 2})
        self.assertEqual({i["rule"] for i in report["issues"]}, {"file-lines", "component-lines", "nesting"})
        component = next(i for i in report["issues"] if i["rule"] == "component-lines")
        self.assertEqual((component["path"], component["line"]), ("ui/deep.slint", 2))
        nesting = next(i for i in report["issues"] if i["rule"] == "nesting")
        self.assertEqual((nesting["path"], nesting["line"]), ("ui/deep.slint", 4))
        self.assertIn("Main: brace nesting 3 exceeds 2", nesting["message"])

    def test_default_nesting_limit_accepts_five_and_rejects_six(self):
        self.assertEqual(LIMITS["nesting"], 5)
        for depth in (5, 6):
            with self.subTest(depth=depth):
                self.source("deep.slint", "export component Deep inherits Rectangle {\n"
                            + "Rectangle {\n" * (depth - 1) + "}\n" * depth)
                report = check(self.root)
                self.assertEqual(report["files"][0]["components"][0]["nesting"], depth)
                self.assertEqual(len(report["issues"]), int(depth > 5))
                if depth > 5:
                    self.assertEqual(report["issues"][0]["line"], 6)

    def test_globals_have_independent_size_and_member_limits(self):
        self.source("state.slint", '''// Shared state
export global Session {
    in-out property <bool> logged-in;
    callback sign-out();
    public function clear() { self.logged-in = false; }
}''')
        report = check(self.root, {"global_lines": 4, "global_members": 2})
        self.assertEqual({i["rule"] for i in report["issues"]}, {"global-lines", "global-members"})
        self.assertTrue(all(i["line"] == 2 for i in report["issues"]))
        metric = report["files"][0]["globals"][0]
        self.assertEqual((metric["name"], metric["code_lines"], metric["members"]), ("Session", 5, 3))
        self.assertEqual(report["files"][0]["components"], [])

    def test_compacting_globals_cannot_bypass_member_limit(self):
        for count in (LIMITS["global_members"], LIMITS["global_members"] + 1):
            with self.subTest(count=count):
                self.source("state.slint", "export global State { "
                            + " ".join(f"in-out property <int> value-{i};" for i in range(count)) + " }")
                report = check(self.root)
                self.assertEqual(report["files"][0]["globals"][0]["members"], count)
                self.assertEqual([i["rule"] for i in report["issues"]],
                                 ["global-members"] if count > LIMITS["global_members"] else [])

    def test_global_nesting_and_metrics_ignore_comments_and_strings(self):
        self.source("state.slint", '''export global Session {
    // property <string> fake; callback fake();
    in-out property <string> text: "global Fake { callback fake(); }";
    public function clear() { if true { if true { if true { if true {} } } } }
}''')
        report = check(self.root)
        self.assertEqual(report["files"][0]["globals"][0]["members"], 2)
        self.assertEqual([i["rule"] for i in report["issues"]], ["nesting"])
        self.assertIn("Session: brace nesting 6", report["issues"][0]["message"])
        self.assertEqual(report["issues"][0]["line"], 4)

    def test_nesting_counts_callbacks_and_reports_each_component_once(self):
        source = '''export component First inherits Rectangle {
    Rectangle { Rectangle { TouchArea {
        clicked => { if true { if false {} } }
    } } }
}
export component Second inherits Rectangle {
    Rectangle { Rectangle { Rectangle { Rectangle { Text {} } } } }
}
export component Flat inherits Rectangle { Rectangle {} Rectangle {} }
'''
        self.source("callbacks.slint", source)
        report = check(self.root)
        self.assertEqual([i["rule"] for i in report["issues"]], ["nesting", "nesting"])
        self.assertEqual([i["line"] for i in report["issues"]], [3, 7])
        self.assertEqual([c["nesting"] for c in report["files"][0]["components"]], [7, 6, 2])
        self.assertIn("First", report["issues"][0]["message"])
        self.assertIn("Second", report["issues"][1]["message"])
        # Compacting the source onto a single line cannot bypass the limit.
        self.source("callbacks.slint", source.replace("\n", " "))
        self.assertEqual(len(check(self.root)["issues"]), 2)

    def test_cli_fails_for_excessive_nesting(self):
        self.source("deep.slint", "export component Deep inherits Rectangle {"
                    + "Rectangle {" * 5 + "}" * 6)
        result = subprocess.run([sys.executable, str(ROOT / "scripts/check_ui_quality.py"),
                                 "--root", str(self.root), "--json"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["issues"][0]["rule"], "nesting")

    def test_shared_imports_and_standard_widgets_are_valid(self):
        self.source("parts/control.slint", "export component Control inherits Rectangle {}")
        self.source("main.slint", '''import { Button } from "std-widgets.slint";
import { Control } from "parts/control.slint";
export component Main inherits Rectangle { Control {} }''')
        self.assertEqual(self.rules(), set())

    def test_missing_imports_and_reexport_cycles_fail(self):
        self.source("a.slint", 'export { B } from "b.slint";\nimport { C } from "missing.slint";')
        self.source("b.slint", 'export { A } from "a.slint";')
        self.assertEqual(self.rules(), {"import-missing", "import-cycle"})

    def test_duplicates_ignore_comments_and_whitespace_but_keep_literals(self):
        self.source("a.slint", 'export component A inherits Rectangle { Text { text: "same"; font-size: 12px; } }')
        self.source("b.slint", 'export component B inherits Rectangle {\n// whitespace\nText { text:"same"; font-size:12px; }\n}')
        issues = check(self.root, {"duplicate_tokens": 12})["issues"]
        self.assertEqual(len(issues), 1, "nested copies should produce one actionable finding")
        self.assertEqual(issues[0]["rule"], "duplicate-block")
        self.assertIn("ui/a.slint:1", issues[0]["message"])
        self.source("b.slint", 'export component B inherits Rectangle { Text { text: "different"; font-size: 12px; } }')
        self.assertEqual(self.rules(duplicate_tokens=12), set())

    def test_duplicates_in_the_same_component_are_detected(self):
        self.source("a.slint", 'export component A inherits Rectangle { Text { text: "same"; } Text { text: "same"; } }')
        self.assertEqual(self.rules(duplicate_tokens=7), {"duplicate-block"})

    def test_unbalanced_braces_and_unterminated_strings_or_comments_fail(self):
        for code, rule in [('component A inherits Rectangle {', 'braces'),
                           ('component A inherits Rectangle {} }', 'braces'),
                           ('Text { text: "unfinished', 'lexical'),
                           ('/* unfinished', 'lexical')]:
            with self.subTest(code=code):
                self.source("broken.slint", code)
                self.assertIn(rule, self.rules())

    def test_empty_tree_and_cli_exit_codes_do_not_silently_pass(self):
        command = [sys.executable, str(ROOT / "scripts/check_ui_quality.py"), "--root", str(self.root), "--json"]
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["issues"][0]["rule"], "sources")
        self.source("good.slint", 'export component Good inherits Rectangle {}')
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(json.loads(result.stdout)["issues"], [])

    def test_imports_cannot_escape_the_project(self):
        (self.root / "outside.slint").write_text("", encoding="utf-8")
        # Use a second project root to ensure the target is outside the scanned project.
        project = self.root / "nested"
        (project / "ui").mkdir(parents=True)
        (project / "ui/main.slint").write_text('import { X } from "../../outside.slint";', encoding="utf-8")
        self.assertEqual(check(project)["issues"][0]["rule"], "import-boundary")

    def test_fractional_geometry_reports_bindings_and_custom_constants(self):
        self.source("layout.slint", '''export component Layout inherits Rectangle {
    x: 12.25px;
    property <length> copied-height: 36.8px;
    width: root.width < 900px ? 33.33% : parent.width;
    padding: .5px;
    height: Math.max(20px, 42.01px);
    callback moved();
    moved => { self.y = 4.25px; }
}''')
        issues = check(self.root)["issues"]
        self.assertEqual([i["line"] for i in issues], [2, 3, 4, 5, 6, 8])
        self.assertEqual({i["rule"] for i in issues}, {"fractional-layout"})

    def test_content_measurements_and_dimensionless_ratios_are_valid(self):
        self.source("layout.slint", '''export component Layout inherits Rectangle {
    // Old screenshot: x: 12.25px; height: 36.8px;
    Text { text: "width: 33.33%"; }
    height: label.preferred-height + 16px;
    width: parent.width / 3;
    font-size: 14px;
    line-height-factor: 1.6;
    opacity: 0.75;
    animate x { duration: 200ms; easing: cubic-bezier(0.2, 0, 0.2, 1); }
    callback moved(length);
    moved(x) => { if Math.abs(x - self.x) > 0.05px { return; } }
}''')
        self.assertEqual(self.rules(), set())

    def test_list_mutations_must_go_through_controller_actions(self):
        self.source("button.slint", '''export component Button inherits Rectangle {
    callback clicked();
    clicked => {
        AppState.queue-songs = [];
        AppState.local-tracks[selection[index]].name = "changed";
        AppState.playlist-tracks[0].id += 1;
    }
}''')
        issues = check(self.root)["issues"]
        self.assertEqual([i["line"] for i in issues], [4, 5, 6])
        self.assertEqual({i["rule"] for i in issues}, {"model-mutation"})

    def test_list_reads_comments_and_action_calls_are_valid(self):
        self.source("button.slint", '''export component Button inherits Rectangle {
    // AppState.queue-songs = [];
    Text { text: "AppState.local-tracks = [];"; }
    callback clicked();
    clicked => {
        if AppState.queue-songs.length == 0 { return; }
        AppState.queue-action("clear-queue", -1);
        AppState.clear-local-library();
    }
}''')
        self.assertEqual(self.rules(), set())

    def test_domain_states_and_aliases_cannot_bypass_model_mutation_rule(self):
        self.source("button.slint", '''export component Button inherits Rectangle {
    callback clicked();
    clicked => {
        QueueState.queue-songs = [];
        LibraryState.local-tracks[selection[index]].name = "changed";
        PlaylistState.playlist-tracks[0].id += 1;
        detail.detail-tracks = [];
    }
}''')
        issues = check(self.root)["issues"]
        self.assertEqual([i["line"] for i in issues], [4, 5, 6, 7])
        self.assertEqual({i["rule"] for i in issues}, {"model-mutation"})
        self.assertIn("QueueState.queue-songs", issues[0]["message"])


if __name__ == "__main__":
    unittest.main()
