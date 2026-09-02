"""Mutation check: put each defect back, one at a time, and require that the
test written for it fails.

A test that passes is not evidence until it has been shown to fail on the thing
it claims to catch. Every mutation is read back off disk before the test runs,
because a patch that quietly did nothing looks exactly like a fix that works.

    python tools/mutate.py
"""

import io
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# (name, file, text to find, text to put in its place, the test that must fail)
MUTATIONS = [
    (
        "heading rows ignore the markers they dropped",
        "src/gui/layout.rs",
        "                    offset: p.offset + line.offset + dropped,",
        "                    offset: p.offset + line.offset,",
        "gui::layout::tests::a_heading_row_points_past_the_markers_it_dropped",
    ),
    (
        "pagination does not skip a blank row at the point it resumes",
        "src/gui/layout.rs",
        "    let mut start = from;\n"
        "    while start < rows.len() && rows[start].kind == RowKind::Blank {\n"
        "        start += 1;\n"
        "    }\n"
        "    let mut used = 0.0f32;\n"
        "    let mut i = start;",
        "    let mut start = from;\n"
        "    let mut used = 0.0f32;\n"
        "    let mut i = from;",
        "gui::layout::tests::a_book_paginates_the_same_however_fast_it_was_set",
    ),
    (
        "the terminal wrap gives split rows the word's offset",
        "src/text.rs",
        "                    cur_start = start + placed;\n"
        "                    limit = width;",
        "                    cur_start = start;\n"
        "                    limit = width;",
        "text::tests::every_wrapped_line_points_at_the_text_it_holds",
    ),
    (
        "the search runs on the wrapped rows again (folding dropped)",
        "src/text.rs",
        "                    hay.push(c.to_ascii_lowercase());",
        "                    hay.push(c);",
        "text::tests::search_matches_are_the_text_they_claim_to_be",
    ),
    (
        "matches_in loses the match that starts exactly at the range end",
        "src/text.rs",
        "    let to = from + matches[from..].partition_point(|m| m.start < end);",
        "    let to = from + matches[from..].partition_point(|m| m.start < end.saturating_sub(1));",
        "text::tests::matches_in_returns_exactly_the_ones_that_touch_the_range",
    ),
    (
        "invisible characters are left inside the words",
        "src/text.rs",
        "        let raw: String = if raw.chars().any(is_invisible) {",
        "        let raw: String = if false {",
        "text::tests::a_soft_hyphen_does_not_hide_a_word_from_the_search",
    ),
    (
        "the verbatim path prefix reaches the library key again",
        "src/library.rs",
        '    match s.strip_prefix(r"\\\\?\\") {',
        '    match None::<&str> {',
        "library::tests::one_file_gets_one_key_however_it_was_named",
    ),
    (
        "an old library is not refiled, so the book stays split in two",
        "src/library.rs",
        "        lib.rekey();",
        "        // lib.rekey();",
        "library::tests::an_old_library_is_refiled_without_losing_anyones_marks",
    ),
    (
        "decomposed text is left as it was written",
        "src/text.rs",
        "        let raw = to_nfc(raw);",
        "        // let raw = to_nfc(raw);",
        "text::tests::text_is_composed_so_one_letter_is_one_character",
    ),
    (
        "the line breaker fills each line in turn instead of looking at the paragraph",
        "src/gui/linebreak.rs",
        "    for tolerance in [MAX_RATIO, 10.0, 1000.0] {",
        "    for tolerance in [] {",
        "gui::linebreak::tests::looking_at_the_paragraph_beats_filling_each_line",
    ),
    (
        "a hyphenated line keeps the hyphen out of its own width",
        "src/gui/linebreak.rs",
        "                width: t.width[i],",
        "                width: t.width[i] + extra,",
        "gui::linebreak::tests::a_hyphenated_line_pays_for_its_own_dash",
    ),
    (
        "lines with nothing on them are kept, and claim to start at offset zero",
        "src/gui/linebreak.rs",
        "    lines.retain(|l| !l.text.is_empty());",
        "    lines.retain(|_l| true);",
        "qa",
    ),
    (
        "hyphenation is asked about words long enough to panic it",
        "src/gui/linebreak.rs",
        "        && word.len() <= hypher::MAX_INLINE_SIZE",
        "        && word.len() <= usize::MAX",
        "gui::linebreak::tests::a_word_too_long_for_the_hyphenator_does_not_take_the_reader_down",
    ),
    (
        "a word split across lines loses its hyphen",
        "src/gui/linebreak.rs",
        "        hyphen: matches!(stream.get(to), Some(Item::Penalty { width, .. }) if *width > 0.0),",
        "        hyphen: false,",
        "gui::linebreak::tests::a_word_split_across_lines_carries_a_hyphen",
    ),
    (
        "an RTF footnote goes the way of the font table again",
        "src/import.rs",
        r'const RTF_NOTES: &[&str] = &["footnote"];',
        r'const RTF_NOTES: &[&str] = &[];',
        "import::note_tests::an_rtf_footnote_is_kept_and_set_at_the_end",
    ),
    (
        "the note editor is not the box the keyboard guard knows about",
        "src/gui/app.rs",
        "        if ctx.text_edit_focused() {",
        "        if ctx.text_edit_focused() && self.show_search {",
        "gui::app::tests::a_note_takes_the_keyboard_away_from_the_shortcuts",
    ),
    (
        "Escape does not reach the note, so it closes something else",
        "src/gui/app.rs",
        "                    if self.note_editor.is_some() {\n"
        "                        self.note_editor = None;\n"
        "                    } else if self.selection.is_some() {",
        "                    if self.selection.is_some() {",
        "gui::app::tests::a_note_takes_the_keyboard_away_from_the_shortcuts",
    ),
    (
        "closing the search leaves the query and its hits behind",
        "src/gui/app.rs",
        "                        self.close_search();",
        "                        self.show_search = false;",
        "gui::app::tests::escape_puts_the_search_away_and_takes_the_query_with_it",
    ),
    (
        "the wheel is the page's wherever it was rolled",
        "src/gui/app.rs",
        "        if !self.page_area.contains(pos) {\n            return false;\n        }",
        "        if false {\n            return false;\n        }",
        "gui::app::tests::the_wheel_belongs_to_whatever_it_was_rolled_over",
    ),
    (
        "a window over the page does not keep what is rolled onto it",
        "src/gui/app.rs",
        "        ctx.layer_id_at(pos)\n"
        "            .is_none_or(|l| l.order == egui::Order::Background)",
        "        true",
        "gui::app::tests::the_wheel_belongs_to_whatever_it_was_rolled_over",
    ),
    (
        "chapter ticks are placed by page number while scrolling",
        "src/gui/app.rs",
        "        let top = layout.tops.get(row).copied().unwrap_or(0.0);\n"
        "        return (top / max).clamp(0.0, 1.0);",
        "        let _ = max;\n"
        "        return layout.page_of_row(row) as f32 / layout.pages.len().max(1) as f32;",
        "gui::app::tests::chapter_ticks_spread_along_the_rule_when_scrolling",
    ),
    (
        "chapter ticks are drawn on a different scale than the knob",
        "src/gui/app.rs",
        "    let page = layout.page_of_row(row) / step * step;\n"
        "    (page.min(last) as f32 / last as f32).clamp(0.0, 1.0)",
        "    (layout.page_of_row(row) as f32 / layout.pages.len().max(1) as f32).clamp(0.0, 1.0)",
        "gui::app::tests::a_tick_is_where_the_knob_will_be",
    ),
    (
        "a toolbar button is named after its glyph again",
        "src/gui/app.rs",
        "    resp.widget_info(|| {\n"
        "        egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, hint)\n"
        "    });\n",
        "",
        "gui::app::tests::the_toolbar_buttons_tell_a_screen_reader_what_they_are",
    ),
    (
        "the night highlighter goes back under the contrast the body is held to",
        "src/gui/theme.rs",
        "        (Ink::Yellow, true) => Color32::from_rgb(0x4A, 0x3E, 0x14),",
        "        (Ink::Yellow, true) => Color32::from_rgb(0x6A, 0x5A, 0x22),",
        "qa",
    ),
    (
        "the terminal help offers `b` as the way back again",
        "src/ui.rs",
        '    ("← h PgUp Bksp", "turn back a page"),',
        '    ("← h PgUp b*", "turn back a page"),',
        "bin/antilib::ui::tests::the_help_never_offers_a_key_that_does_something_else",
    ),
    (
        "the row positions stop skipping combining marks",
        "src/gui/app.rs",
        "            if !crate::text::is_combining(c) {\n"
        "                x += f.glyph_width(font, c);\n"
        "            }",
        "            x += f.glyph_width(font, c);",
        "gui::app::tests::batching_the_font_lock_did_not_move_a_single_glyph",
    ),
    (
        "a damaged library is thrown away instead of being set aside",
        "src/library.rs",
        "        if std::fs::rename(path, &target).is_ok() {",
        "        if std::fs::remove_file(path).is_ok() {",
        "tests/durability.rs::a_damaged_store_is_not_written_over_by_an_empty_one",
    ),
    (
        "a save writes this reader's whole copy over the file again",
        "src/library.rs",
        """                Ok(mut disk) => {
                    disk.rekey();
                    merge_books(&self.baseline, &self.books, disk.books)
                }""",
        """                Ok(_) => self.books.clone(),""",
        "tests/durability.rs::two_readers_open_at_once_do_not_erase_each_others_marks",
    ),
    (
        "the merge takes the union, so an erased highlight comes back",
        "src/library.rs",
        """            let deleted_here = base.is_some_and(|r| r.highlights.iter().any(same));
            let already_here = self.highlights.iter().any(same);""",
        """            let deleted_here = false;
            let already_here = self.highlights.iter().any(same);""",
        "tests/durability.rs::a_deleted_highlight_does_not_come_back_from_the_file_on_disk",
    ),
    (
        "the library is renamed into place without ever reaching the disk",
        "src/library.rs",
        "        f.sync_all()?;",
        "        let _ = &f;",
        "tests/durability.rs::a_saved_library_is_on_the_disk_before_the_name_is_swapped",
    ),
    (
        "every key is resolved on the filesystem at every start again",
        "src/library.rs",
        "            .filter(|k| !looks_settled(k))",
        "            .filter(|_| true)",
        "tests/durability.rs::a_library_of_unreachable_books_still_opens_promptly",
    ),
    (
        "a zip entry is read whole, however much it turns out to be",
        "src/import.rs",
        """    if buf.len() as u64 > MAX_ENTRY_BYTES {
        return Err(too_large(&format!("{name} inside this document"), MAX_ENTRY_BYTES));
    }""",
        """    if false {
        return Err(too_large(&format!("{name} inside this document"), MAX_ENTRY_BYTES));
    }""",
        "import::bomb_tests::an_archive_that_unpacks_to_too_much_is_refused",
    ),
    (
        "a chapter refused for its size is reported as a missing chapter",
        "src/import.rs",
        """            Err(e) if is_too_large(&e) => return Err(e),
            Err(_) => {
                missing.push(href.clone());""",
        """            Err(_) => {
                missing.push(href.clone());""",
        "import::bomb_tests::an_epub_refused_for_size_says_so_and_not_that_a_chapter_is_missing",
    ),
    (
        "the terminal listing goes back to alphabetical order by path",
        "src/bin/antilib.rs",
        "    lib.recent()\n        .into_iter()",
        "    lib.books\n        .iter()",
        "bin/antilib::tests::the_recent_listing_puts_the_last_book_read_first",
    ),
    (
        "the terminal cleanup goes back to running only on the way out",
        "src/bin/antilib.rs",
        "    let _guard = TerminalGuard;\n",
        "",
        "tests/durability.rs::the_terminal_is_handed_back_however_the_reader_leaves",
    ),
    (
        "the merge resurrects a deleted bookmark",
        "src/library.rs",
        "            let deleted_here = base.is_some_and(|r| r.bookmarks.iter().any(|m| m.offset == b.offset));",
        "            let deleted_here = false;",
        "qa",
    ),
    (
        "a combining mark is measured as a character of its own width",
        "src/text.rs",
        "    unicode_width::UnicodeWidthChar::width(c) == Some(0) && !c.is_control()",
        "    false && !c.is_control()",
        "tests/painted_width.rs::the_measured_width_is_the_painted_width",
    ),
]


def read(path):
    with io.open(os.path.join(ROOT, path), encoding="utf-8") as f:
        return f.read()


def write(path, text):
    with io.open(os.path.join(ROOT, path), "w", encoding="utf-8", newline="") as f:
        f.write(text)


# Where the untouched source is kept while a mutation is in place.
#
# `finally` covers an exception and not a kill, and this script has been killed
# mid-mutation. The journal is what makes the damage recoverable: it is written
# before the source is changed and removed only after the source is read back
# and found to match.
JOURNAL = os.path.join(ROOT, ".mutation-journal.json")


def journal_write(path, original):
    with io.open(JOURNAL, "w", encoding="utf-8", newline="") as f:
        json.dump({"path": path, "original": original}, f)


def journal_clear():
    try:
        os.remove(JOURNAL)
    except OSError:
        pass


def journal_restore():
    """Put back whatever a killed run left behind. Returns what it restored."""
    if not os.path.exists(JOURNAL):
        return None
    try:
        with io.open(JOURNAL, encoding="utf-8") as f:
            entry = json.load(f)
        path, original = entry["path"], entry["original"]
    except (OSError, ValueError, KeyError):
        return None
    current = read(path)
    if current != original:
        write(path, original)
        journal_clear()
        return path
    journal_clear()
    return None


def run_gate():
    """Run the quality gate. Returns (passed, output).

    Some defects are only visible across the matrix — the gate found six of
    them — and a mutation that only the gate can see is still a mutation worth
    checking. Its exit code is the verdict, and unlike a test filter it cannot
    silently match nothing.
    """
    r = subprocess.run(
        ["cargo", "run", "--release", "--quiet", "--bin", "antilib-qa"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    out = r.stdout + r.stderr
    if "checks" not in out:
        raise AssertionError("the gate did not run: " + out[-2000:])
    return r.returncode == 0, out


def run_test(name):
    """Run one test. Returns (passed, output).

    Cargo exits 0 when a filter matches nothing, so a mistyped test name looks
    exactly like a test that passed — which is how the first run of this script
    reported three real defects as uncaught. The count is therefore read out of
    the summary and required to be one.

    A name of the form `tests/file.rs::test` runs an integration test, and one
    of the form `bin/name::test` runs a test that lives in a binary — the
    terminal reader's own modules are not part of the library, so `--lib` finds
    nothing there and cargo calls that a pass.
    """
    if name == "qa":
        return run_gate()
    if "::" in name and name.startswith("tests/"):
        target, name = name.split("::", 1)
        target = target[len("tests/"):].removesuffix(".rs")
        where = ["--test", target]
    elif "::" in name and name.startswith("bin/"):
        target, name = name.split("::", 1)
        where = ["--bin", target[len("bin/"):]]
    else:
        where = ["--lib"]
    r = subprocess.run(
        ["cargo", "test"] + where + [name, "--", "--exact"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    out = r.stdout + r.stderr
    ran = re.search(r"(\d+) passed; (\d+) failed", out)
    if not ran:
        raise AssertionError("no test summary for %s: %s" % (name, out[-2000:]))
    passed, failed = int(ran.group(1)), int(ran.group(2))
    if passed + failed != 1:
        raise AssertionError(
            "%s matched %d tests, not 1 — check the module path" % (name, passed + failed)
        )
    return failed == 0, out


def main():
    recovered = journal_restore()
    if recovered:
        print("A previous run was killed with a mutation in place.")
        print("Restored %s from the journal.\n" % recovered)
    print("Checking that the tests fail on the defects they were written for.\n")
    caught, missed = 0, []
    for name, path, find, replace, test in MUTATIONS:
        original = read(path)
        if original.count(find) != 1:
            missed.append((name, "the mutation does not apply to the current source"))
            print("  ??  %s\n      anchor found %d times" % (name, original.count(find)))
            continue

        # A green run before the mutation, or the result below means nothing.
        ok, _ = run_test(test)
        if not ok:
            missed.append((name, "the test was already failing"))
            print("  ??  %s\n      %s was red before the mutation" % (name, test))
            continue

        journal_write(path, original)
        write(path, original.replace(find, replace, 1))
        try:
            # Never trust a patch you have not read back.
            on_disk = read(path)
            assert replace in on_disk and find not in on_disk, "the mutation did not reach the disk"
            ok, output = run_test(test)
        finally:
            write(path, original)
            assert read(path) == original, "the source was not restored"
            journal_clear()

        if ok:
            missed.append((name, "%s still passed" % test))
            print("  NO  %s\n      %s did not notice" % (name, test))
        else:
            caught += 1
            print("  ok  %s\n      caught by %s" % (name, test))

    print("\n%d of %d mutations caught." % (caught, len(MUTATIONS)))
    if missed:
        print("\nNot caught:")
        for name, why in missed:
            print("  - %s: %s" % (name, why))
        sys.exit(1)


if __name__ == "__main__":
    main()
