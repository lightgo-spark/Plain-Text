//! Replays each symptom the audit reported, from a real file through the
//! public API — the way the reader meets them.

use anti_library::import::rtf_to_text;
use anti_library::text::Document;
use std::path::PathBuf;

fn dir() -> PathBuf {
    let d = std::env::temp_dir().join("antilib-verify");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn body(d: &Document) -> String {
    d.paragraphs
        .iter()
        .map(|p| p.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn symptom_1_entities_survive_an_html_file() {
    let p = dir().join("ent.html");
    std::fs::write(
        &p,
        "<html><body><p>R&amp;D team&#8217;s caf&#233; &mdash; 5 &lt; 6</p></body></html>",
    )
    .unwrap();
    let out = body(&Document::load(&p).unwrap());
    println!("1  entities   -> {out:?}");
    assert_eq!(out.trim(), "R&D team\u{2019}s caf\u{e9} \u{2014} 5 < 6");
}

#[test]
fn symptom_2_a_euc_kr_page_opens() {
    let (euc, _, _) =
        encoding_rs::EUC_KR.encode("<html><body><p>한글 문서입니다.</p></body></html>");
    let p = dir().join("korean.html");
    std::fs::write(&p, &euc).unwrap();
    let out = body(&Document::load(&p).expect("an EUC-KR page must open"));
    println!("2  euc-kr html-> {out:?}");
    assert_eq!(out.trim(), "한글 문서입니다.");
}

#[test]
fn symptom_3_a_notepad_unicode_file_opens() {
    let mut bytes = vec![0xFFu8, 0xFE];
    for u in "안녕하세요\r\n반갑습니다".encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let p = dir().join("utf16.txt");
    std::fs::write(&p, &bytes).unwrap();
    let d = Document::load(&p).expect("a UTF-16 text file must open");
    println!("3  utf-16 txt -> {:?} as {}", body(&d), d.encoding);
    assert_eq!(d.paragraphs[0].text, "안녕하세요");
    assert_eq!(d.paragraphs[1].text, "반갑습니다");
}

#[test]
fn symptom_4_an_epub_linking_by_url_keeps_its_chapters() {
    let p = dir().join("t.epub");
    let f = std::fs::File::create(&p).unwrap();
    let mut z = zip::ZipWriter::new(f);
    use std::io::Write;
    let o = zip::write::SimpleFileOptions::default();
    z.start_file("META-INF/container.xml", o).unwrap();
    z.write_all(
        br#"<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#,
    )
    .unwrap();
    z.start_file("OEBPS/content.opf", o).unwrap();
    z.write_all(
        br#"<package><manifest><item id="c1" href="chapter%201.xhtml"/></manifest>
            <spine><itemref idref="c1"/></spine></package>"#,
    )
    .unwrap();
    z.start_file("OEBPS/chapter 1.xhtml", o).unwrap();
    z.write_all(b"<html><body><p>Chapter body text.</p></body></html>")
        .unwrap();
    z.finish().unwrap();
    let out = body(&Document::load(&p).expect("the %20 link must be followed"));
    println!("4  epub %20   -> {out:?}");
    assert!(out.contains("Chapter body text."), "{out}");
}

#[test]
fn symptom_7_a_slice_is_as_long_as_it_was_asked_for() {
    let src = "First line has trailing spaces   \nsecond line.\n";
    let d = Document::from_string(src.to_string(), &PathBuf::from("t.txt"), "UTF-8");
    let all: Vec<char> = src.chars().collect();
    for (a, b) in [(0usize, 10usize), (5, 36), (30, 40), (0, all.len())] {
        let expected: String = all[a..b].iter().collect();
        assert_eq!(d.slice(a, b), expected, "slice {a}..{b}");
    }
    println!("7  slice      -> exact for every range");
}

#[test]
fn symptom_9_and_10_rtf_reads_its_own_declarations() {
    // Built from pieces so no backslash-escape sequence sits in this source.
    let bs = '\\';
    let latin = format!("{{{bs}rtf1{bs}ansi{bs}ansicpg1252 caf{bs}'e9 na{bs}'efve{bs}par}}");
    println!("9  rtf 1252   -> {:?}", rtf_to_text(&latin).trim());
    assert_eq!(rtf_to_text(&latin).trim(), "caf\u{e9} na\u{ef}ve");

    let two = format!(
        "{{{bs}rtf1{bs}ansi{bs}uc2 {bs}u51069?_{bs}u51648?_ {bs}u52293?_.{bs}par}}"
    );
    println!("10 rtf uc2    -> {:?}", rtf_to_text(&two).trim());
    assert_eq!(rtf_to_text(&two).trim(), "읽지 책.");
}
