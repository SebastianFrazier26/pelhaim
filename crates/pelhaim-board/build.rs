#[path = "src/font/grid.rs"]
mod grid;

use std::fmt::Write as _;
use std::path::Path;
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=../../assets/font/glyphs.txt");
    let src_path =
        Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../assets/font/glyphs.txt");
    let src =
        fs::read_to_string(&src_path).unwrap_or_else(|e| panic!("{}: {e}", src_path.display()));
    let cuts = grid::parse(&src).unwrap_or_else(|e| panic!("assets/font/glyphs.txt: {e}"));

    let mut out = String::new();
    for (name, w, h) in [("CUT_5X7", 5, 7), ("CUT_7X9", 7, 9)] {
        let cut = cuts
            .iter()
            .find(|g| g.w == w && g.h == h)
            .unwrap_or_else(|| panic!("assets/font/glyphs.txt: missing cut [{w}x{h}]"));
        let mut map = cut.map.clone();
        // Sorted so the lib can binary-search.
        map.sort_by_key(|(c, _)| *c);
        writeln!(out, "static {name}: &[(char, [u16; {h}])] = &[").unwrap();
        for (c, rows) in &map {
            let rows: Vec<String> = rows.iter().map(|r| format!("{r:#06x}")).collect();
            writeln!(
                out,
                "    ('\\u{{{:x}}}', [{}]),",
                *c as u32,
                rows.join(", ")
            )
            .unwrap();
        }
        writeln!(out, "];").unwrap();
    }
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("glyphs.rs"),
        out,
    )
    .unwrap();
}
