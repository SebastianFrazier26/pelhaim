use std::fs;
use std::path::Path;

use chrono::NaiveDate;
use pelhaim_core::{Decision, DecisionId, Origin, Who};
use pelhaim_store::decision_log::{
    DecisionLogError, LogProject, insert_entry, open_decisions, render_entry, write_decision,
    write_decision_with_hook,
};

const BASE: &str = include_str!("goldens/base.md");
const EXPECTED_TOP: &str = include_str!("goldens/expected_top.md");

fn decision(id: i64, title: &str, why: &str) -> Decision {
    Decision {
        id: DecisionId(id),
        project: "pelhaim".into(),
        date: NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(),
        title: title.into(),
        why: why.into(),
        who: Who::Owner,
        origin: Origin::Local,
        source_alert: None,
        written_to_md: false,
        project_name: None,
    }
}

fn c1() -> Decision {
    decision(7, "Stage C1: Proceed", "Proceed to C1?")
}

fn project(slug: &str, name: &str, log_section: Option<&str>) -> LogProject {
    LogProject::new(slug, name, log_section)
}

fn pelhaim() -> LogProject {
    project("pelhaim", "pelhaim", None)
}

/// Asserts `out` is `doc` with exactly `inserted` spliced in at one offset, and returns that offset.
fn assert_pure_insert(doc: &str, out: &str, inserted: &str) -> usize {
    assert_eq!(out.len(), doc.len() + inserted.len(), "{out:?}");
    let at = out.find(inserted).expect("inserted text present");
    let mut rebuilt = String::new();
    rebuilt.push_str(&out[..at]);
    rebuilt.push_str(&out[at + inserted.len()..]);
    assert_eq!(rebuilt, doc, "bytes outside the insertion changed");
    at
}

fn temp_log(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("DECISION-LOG.md");
    fs::write(&path, content).unwrap();
    (dir, path)
}

fn leftover_temp_files(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("pelhaim-tmp"))
        .collect()
}

#[test]
fn entry_renders_exact_format() {
    assert_eq!(
        render_entry(&c1(), "\n"),
        "- **2026-11-23: Stage C1: Proceed**\n  - Why: Proceed to C1?\n  - Who: owner. <!-- pelhaim:decision:7:137c9edb -->\n\n"
    );
    assert_eq!(
        render_entry(&c1(), "\r\n"),
        "- **2026-11-23: Stage C1: Proceed**\r\n  - Why: Proceed to C1?\r\n  - Who: owner. <!-- pelhaim:decision:7:137c9edb -->\r\n\r\n"
    );
}

#[test]
fn entry_sanitizes_title_and_why() {
    let mut d = decision(3, "Two\nlines\x1b[31m red", "why\r\n- not a bullet");
    d.who = Who::AgentDefaultUnconfirmed;
    assert_eq!(
        render_entry(&d, "\n"),
        "- **2026-11-23: Two lines red**\n  - Why: why - not a bullet\n  - Who: agent default, not yet confirmed. <!-- pelhaim:decision:3:0da8363b -->\n\n"
    );
}

#[test]
fn inserts_at_top_of_decided() {
    let out = insert_entry(BASE, &pelhaim(), &render_entry(&c1(), "\n"));
    assert_eq!(out, EXPECTED_TOP);
}

#[test]
fn heading_with_suffix_matches_not_duplicated() {
    let out = insert_entry(BASE, &pelhaim(), &render_entry(&c1(), "\n"));
    let headings: Vec<&str> = out
        .lines()
        .filter(|l| l.starts_with("## pelhaim"))
        .collect();
    assert_eq!(headings, ["## pelhaim (new, 2026-11-23)"]);
    let entry_at = out.find("Stage C1").unwrap();
    assert!(entry_at > out.find("## pelhaim (new").unwrap());
    assert!(entry_at < out.find("## Standing rules").unwrap());
}

#[test]
fn heading_match_needs_a_word_boundary() {
    let doc = "## pelhaimx\n\n### Decided\n\n## Standing rules\n";
    let out = insert_entry(doc, &pelhaim(), "- E\n\n");
    let at = assert_pure_insert(
        doc,
        &out,
        "## pelhaim\n\n### Open decisions (need you)\n\n### Decided\n\n- E\n\n",
    );
    assert_eq!(at, doc.find("## Standing rules").unwrap());
}

#[test]
fn quoted_suffix_and_log_section_override() {
    let doc = "## mlb fan sentiment (\"mlballpark\")\n\n### Open decisions (need you)\n\n### Decided\n\n- **old**\n  - Who: owner.\n\n---\n\n## cabn and the portfolio (sebastianfrazier.com)\n\n### Open decisions (need you)\n\n### Decided\n\n- **older**\n  - Who: owner.\n\n## Standing rules\n";

    let mlb = project("mlb", "MLB Fan Sentiment", None);
    let out = insert_entry(doc, &mlb, "- MLB\n\n");
    let at = assert_pure_insert(doc, &out, "- MLB\n\n");
    assert_eq!(at, doc.find("- **old**").unwrap());

    let cabn = project("cabn", "cabn", Some("cabn and the portfolio"));
    assert_eq!(cabn.section_key, "cabn and the portfolio");
    let out = insert_entry(doc, &cabn, "- CABN\n\n");
    let at = assert_pure_insert(doc, &out, "- CABN\n\n");
    assert_eq!(at, doc.find("- **older**").unwrap());
}

#[test]
fn section_without_decided_gets_one() {
    let doc = "## Alpha\n\n### Open decisions (need you)\n\n- **Q?**\n\n---\n\n## Beta\n\n### Decided\n\n";
    let alpha = project("alpha", "Alpha", None);
    let out = insert_entry(doc, &alpha, "- E\n\n");
    let at = assert_pure_insert(doc, &out, "### Decided\n\n- E\n\n");
    assert_eq!(at, doc.find("---").unwrap());

    let doc = "## Alpha\n\n### Open decisions (need you)\n\n## Beta\n";
    let out = insert_entry(doc, &alpha, "- E\n\n");
    let at = assert_pure_insert(doc, &out, "### Decided\n\n- E\n\n");
    assert_eq!(at, doc.find("## Beta").unwrap());
}

#[test]
fn section_without_decided_at_eof_without_newline() {
    let doc = "## Alpha\n\n### Open decisions (need you)\n\n- **Q?**";
    let out = insert_entry(doc, &project("alpha", "Alpha", None), "- E\n\n");
    let at = assert_pure_insert(doc, &out, "\n### Decided\n\n- E\n\n");
    assert_eq!(at, doc.len());
}

#[test]
fn new_project_inserted_before_standing_rules() {
    let gamma = project("gamma", "Gamma", None);
    let out = insert_entry(BASE, &gamma, "- E\n\n");
    let at = assert_pure_insert(
        BASE,
        &out,
        "## Gamma\n\n### Open decisions (need you)\n\n### Decided\n\n- E\n\n",
    );
    assert_eq!(at, BASE.find("## Standing rules").unwrap());

    let doc = "## Alpha\n\n### Decided\n\n- **old**";
    let out = insert_entry(doc, &gamma, "- E\n\n");
    let at = assert_pure_insert(
        doc,
        &out,
        "\n## Gamma\n\n### Open decisions (need you)\n\n### Decided\n\n- E\n\n",
    );
    assert_eq!(at, doc.len());
}

#[test]
fn crlf_and_no_trailing_newline_preserved() {
    let doc = BASE.replace('\n', "\r\n");
    let doc = doc.trim_end_matches("\r\n");
    let entry = render_entry(&c1(), "\r\n");
    let out = insert_entry(doc, &pelhaim(), &entry);
    let at = assert_pure_insert(doc, &out, &entry);
    assert_eq!(
        at,
        doc.find("- **2026-11-23: New project pelhaim.**").unwrap()
    );

    let doc = "## Alpha\r\n\r\n### Decided";
    let alpha = project("alpha", "Alpha", None);
    let out = insert_entry(doc, &alpha, &entry);
    let inserted = format!("\r\n{entry}");
    let at = assert_pure_insert(doc, &out, &inserted);
    assert_eq!(at, doc.len());
}

#[test]
fn rest_of_file_byte_identical() {
    let entry = render_entry(&c1(), "\n");
    for p in [
        pelhaim(),
        project("alpha", "Alpha", None),
        project("new", "Brand New", None),
    ] {
        let out = insert_entry(BASE, &p, &entry);
        assert!(out.contains(&entry));
        let start = out.find(&entry).unwrap();
        let heading_extra = out.len() - BASE.len() - entry.len();
        let mut stripped = out[..start - heading_extra].to_string();
        stripped.push_str(&out[start + entry.len()..]);
        assert_eq!(stripped, BASE, "project {}", p.slug);
        assert!(
            out.contains("Contents: [Alpha](#alpha) · [pelhaim](#pelhaim-new-2026-11-23) · [Standing rules](#standing-rules)\n")
        );
    }
}

#[test]
fn interleaved_sections_are_not_repaired() {
    let doc = "## Alpha\n\n### Decided\n\n- **a1**\n\n## Beta\n\n### Decided\n\n- **b1**\n\n## Alpha\n\n### Decided\n\n- **a2**\n";
    let out = insert_entry(doc, &project("alpha", "Alpha", None), "- E\n\n");
    let at = assert_pure_insert(doc, &out, "- E\n\n");
    assert_eq!(at, doc.find("- **a1**").unwrap());
}

#[test]
fn symlinked_log_keeps_link() {
    let (dir, real) = temp_log(BASE);
    let link = dir.path().join("link.md");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    write_decision(&link, &pelhaim(), &c1()).unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_link(&link).unwrap(), real);
    assert_eq!(fs::read_to_string(&real).unwrap(), EXPECTED_TOP);
    assert!(dir.path().join("DECISION-LOG.md.pelhaim-bak").exists());
    assert!(!dir.path().join("link.md.pelhaim-bak").exists());
}

#[test]
fn write_preserves_crlf_and_missing_final_newline() {
    let doc = BASE.replace('\n', "\r\n");
    let doc = doc.trim_end_matches("\r\n").to_string();
    let (_dir, path) = temp_log(&doc);

    write_decision(&path, &pelhaim(), &c1()).unwrap();

    let out = fs::read_to_string(&path).unwrap();
    assert_pure_insert(&doc, &out, &render_entry(&c1(), "\r\n"));
}

#[test]
fn concurrent_change_retries_once_then_conflicts() {
    let (dir, path) = temp_log(BASE);
    let mut calls = 0;
    write_decision_with_hook(&path, &pelhaim(), &c1(), &mut |p: &Path| {
        calls += 1;
        if calls == 1 {
            let mut s = fs::read_to_string(p).unwrap();
            s.push_str("orchestrator line\n");
            fs::write(p, s).unwrap();
        }
    })
    .unwrap();
    assert_eq!(calls, 2);
    let out = fs::read_to_string(&path).unwrap();
    assert_eq!(out, format!("{EXPECTED_TOP}orchestrator line\n"));
    assert!(leftover_temp_files(dir.path()).is_empty());

    let (dir, path) = temp_log(BASE);
    let mut calls = 0;
    let err = write_decision_with_hook(&path, &pelhaim(), &c1(), &mut |p: &Path| {
        calls += 1;
        let mut s = fs::read_to_string(p).unwrap();
        s.push_str(&format!("orchestrator line {calls}\n"));
        fs::write(p, s).unwrap();
    })
    .unwrap_err();
    assert_eq!(err, DecisionLogError::Conflict);
    assert_eq!(calls, 2);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("{BASE}orchestrator line 1\norchestrator line 2\n")
    );
    assert!(leftover_temp_files(dir.path()).is_empty());
}

#[test]
fn backup_written() {
    let (dir, path) = temp_log(BASE);
    write_decision(&path, &pelhaim(), &c1()).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("DECISION-LOG.md.pelhaim-bak")).unwrap(),
        BASE
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), EXPECTED_TOP);
}

#[test]
fn missing_file_is_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("DECISION-LOG.md");
    let err = write_decision(&path, &pelhaim(), &c1()).unwrap_err();
    assert_eq!(err, DecisionLogError::NotFound(path.clone()));
    assert!(!path.exists());

    let link = dir.path().join("dangling.md");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    let err = write_decision(&link, &pelhaim(), &c1()).unwrap_err();
    assert_eq!(err, DecisionLogError::NotFound(link));
    assert!(!path.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn crash_replay_does_not_duplicate_entry() {
    let (_dir, path) = temp_log(BASE);
    write_decision(&path, &pelhaim(), &c1()).unwrap();
    // A crash before `DecisionExported` was recorded means the same decision is exported again on restart.
    write_decision(&path, &pelhaim(), &c1()).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), EXPECTED_TOP);

    let other = decision(70, "Stage C2", "Next");
    write_decision(&path, &pelhaim(), &other).unwrap();
    let out = fs::read_to_string(&path).unwrap();
    assert_eq!(
        out.matches("<!-- pelhaim:decision:7:137c9edb -->").count(),
        1
    );
    assert_eq!(
        out.matches("<!-- pelhaim:decision:70:38e51e62 -->").count(),
        1
    );
}

#[test]
fn recycled_id_with_different_content_is_still_written() {
    let (_dir, path) = temp_log(BASE);
    write_decision(&path, &pelhaim(), &c1()).unwrap();
    // After a store reset ids restart, so an unrelated decision can reuse id 7.
    let reused = decision(7, "Stage C2: Hold", "Hold at C1?");
    write_decision(&path, &pelhaim(), &reused).unwrap();
    let out = fs::read_to_string(&path).unwrap();
    assert!(out.contains(&render_entry(&reused, "\n")));
    assert!(out.contains(&render_entry(&c1(), "\n")));
    assert_eq!(out.matches("<!-- pelhaim:decision:7:").count(), 2);
}

#[test]
fn open_decisions_counted_per_project() {
    let ps = [
        project("alpha", "Alpha", None),
        pelhaim(),
        project("gone", "Not In File", None),
    ];
    let open = open_decisions(BASE, &ps);
    assert_eq!(
        open["alpha"],
        [
            "- **Confirm or overturn: alpha reads the filled CSV files** (agent default). See Decided.",
            "- **Later: delete the rehearsal branches.**",
        ]
    );
    assert_eq!(
        open["pelhaim"],
        ["- **Optional: motion clips** (the stills can't show motion;"]
    );
    assert!(open["gone"].is_empty());
    assert_eq!(open.len(), 3);

    let crlf = BASE.replace('\n', "\r\n");
    assert_eq!(open_decisions(&crlf, &ps), open);
}

// A `DecisionLogged` lost to a failed persist leaves the alert unlogged after a
// restart, so the owner can log it again on a later day under a recycled id.
#[test]
fn same_source_alert_logged_again_later_is_not_duplicated() {
    let (_dir, path) = temp_log(BASE);
    let first = Decision {
        source_alert: Some(pelhaim_core::AlertId("q:claude:S1:toolu_q".into())),
        ..c1()
    };
    write_decision(&path, &pelhaim(), &first).unwrap();
    let again = Decision {
        id: DecisionId(1),
        date: NaiveDate::from_ymd_opt(2026, 11, 25).unwrap(),
        ..first.clone()
    };
    write_decision(&path, &pelhaim(), &again).unwrap();
    let out = fs::read_to_string(&path).unwrap();
    assert_eq!(out.matches("Stage C1: Proceed").count(), 1, "{out}");

    let other_alert = Decision {
        source_alert: Some(pelhaim_core::AlertId("q:claude:S2:toolu_r".into())),
        ..again
    };
    write_decision(&path, &pelhaim(), &other_alert).unwrap();
    let out = fs::read_to_string(&path).unwrap();
    assert_eq!(out.matches("Stage C1: Proceed").count(), 2, "{out}");
}

#[test]
fn non_project_headings_never_match_a_project() {
    for (name, heading) in [
        ("Standing", "## Standing rules"),
        ("How", "## How to use this log"),
    ] {
        let p = project("p", name, None);
        let d = decision(9, "Pick: one", "Which?");
        let out = insert_entry(BASE, &p, &render_entry(&d, "\n"));
        let own = out
            .find(&format!("## {name}\n"))
            .expect("own section created");
        let entry = out.find("Pick: one").unwrap();
        assert!(own < entry, "{name}");
        let after_heading = out.find(heading).unwrap();
        assert!(
            !(after_heading < entry && out[after_heading..entry].matches("\n## ").count() == 0),
            "{name}: entry landed under {heading}"
        );
        assert!(open_decisions(BASE, &[p])["p"].is_empty());
    }
}
