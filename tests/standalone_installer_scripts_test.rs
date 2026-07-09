#[test]
fn unix_installer_accepts_current_cargo_dist_root_binary_layout() {
    let script = include_str!("../install.sh");

    assert!(script.contains("found=\"$dest/rustcodegraph-${artifact_target}/rustcodegraph\""));
    assert!(script.contains("mv \"$found\" \"$dest/bin/rustcodegraph\""));
    assert!(!script.contains("$bundle/bin/rustcodegraph"));
}

#[test]
fn windows_installer_accepts_current_cargo_dist_root_binary_layout() {
    let script = include_str!("../install.ps1");

    assert!(script.contains("$exe = Join-Path $dest 'rustcodegraph.exe'"));
    assert!(
        script
            .contains("Move-Item -Path $exe -Destination (Join-Path $binDir 'rustcodegraph.exe')")
    );
    assert!(!script.contains("$bundle"));
    assert!(!script.contains("bin\\rustcodegraph.exe"));
}
