//! @arch:layer(cli)
//! @arch:role(shim)
//!
//! `yahb` — short alias for `yah board ...`. Forwards argv to the sibling
//! `yah` binary with `board` prepended, then exits with its status code.

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let mut exe = std::env::current_exe().expect("current_exe");
    exe.set_file_name(if cfg!(windows) { "yah.exe" } else { "yah" });

    let status = Command::new(&exe)
        .arg("board")
        .args(std::env::args_os().skip(1))
        .status()
        .unwrap_or_else(|e| {
            eprintln!("yahb: failed to invoke {}: {}", exe.display(), e);
            std::process::exit(127);
        });

    ExitCode::from(status.code().unwrap_or(1) as u8)
}
