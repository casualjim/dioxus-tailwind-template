use std::{path::PathBuf, process::Command};

#[cfg(windows)]
pub const CARGO: &str = "cargo.cmd";

#[cfg(not(windows))]
pub const CARGO: &str = "cargo";

pub fn main() {
  let path = PathBuf::from_iter([env!("CARGO_MANIFEST_DIR"), "crates/frontend"]);

  Command::new(CARGO)
    .current_dir(path)
    .args(["watch", "-s", "dioxus build"])
    .spawn()
    .unwrap()
    .wait_with_output()
    .unwrap();
}
