use std::process::Command;

#[cfg(windows)]
pub const SYSTEMFD: &str = "systemfd.cmd";

#[cfg(not(windows))]
pub const SYSTEMFD: &str = "systemfd";

pub fn main() {
  let dir = env!("CARGO_MANIFEST_DIR");

  Command::new(SYSTEMFD)
    .current_dir(dir)
    .args([
      "--no-pid",
      "-s",
      "http::[::1]:8080",
      "-s",
      "https::[::1]:8443",
      "--",
      "cargo",
      "watch",
      "-x",
      "run -- --tls-mode=key-pair --tls-key ../../certs/server.key --tls-cert ../../certs/server.crt --static-dir ../frontend/dist/ --https-port 8443 --http-port 8080"
    ])
    .spawn()
    .unwrap()
    .wait_with_output()
    .unwrap();
}
