//! Apps opened from Finder or the Dock start with PATH=/usr/bin:/bin. PandaCode widens its
//! own PATH at startup so the agents and tools it launches can run `gh`, `node` and other
//! Homebrew tools. Its own test binary keeps this process-wide change isolated.
#[cfg(unix)]
#[test]
fn children_of_a_finder_launch_find_homebrew_tools() {
    std::env::set_var("PATH", "/usr/bin:/bin");
    pandacode_lib::prepare_environment();
    let output = std::process::Command::new("sh").args(["-c", "printf %s \"$PATH\""]).output().expect("sh runs");
    let path = String::from_utf8(output.stdout).unwrap();
    let dirs: Vec<&str> = path.split(':').collect();
    assert_eq!(&dirs[..2], ["/usr/bin", "/bin"], "existing entries keep their order");
    for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
        assert!(dirs.contains(&dir), "{dir} is on the PATH children inherit: {path}");
    }
    let home = std::env::var("HOME").unwrap();
    assert!(dirs.contains(&format!("{home}/.local/bin").as_str()));
}
