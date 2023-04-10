fn main() -> std::io::Result<()> {
  let out_dir =
    std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or(std::io::ErrorKind::NotFound)?);

  let cmd = clap::Command::new("{{project-name}}d")
    .arg(clap::arg!(-n --name <NAME>))
    .arg(clap::arg!(-c --count <NUM>));

  let man = clap_mangen::Man::new(cmd);
  let mut buffer: Vec<u8> = Default::default();
  man.render(&mut buffer)?;

  std::fs::write(out_dir.join("{{project-name}}d.1"), buffer)?;

  // trigger recompilation when a new migration is added
  println!("cargo:rerun-if-changed=migrations");
  Ok(())
}
