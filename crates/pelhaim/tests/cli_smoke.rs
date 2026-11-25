#[test]
fn version_flag_prints_name() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pelhaim"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("pelhaim "));
}
