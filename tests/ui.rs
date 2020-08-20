use assert_cmd::prelude::*;

use std::process::Command;

fn main() {
    for entry in glob::glob("examples/*.rs").expect("Failed to read glob pattern") {
        let entry = entry.unwrap();
        let mut cmd = Command::cargo_bin(entry.with_extension("").to_str().unwrap())
            .unwrap();
        let output = cmd.unwrap();
        if output.stderr.is_empty() {
            let _ = std::fs::remove_file(entry.with_extension("stderr"));
        } else {
            std::fs::write(entry.with_extension("stderr"), output.stderr).unwrap();
        }
        if output.stdout.is_empty() {
            let _ = std::fs::remove_file(entry.with_extension("stdout"));
        } else {
            std::fs::write(entry.with_extension("stdout"), output.stdout).unwrap();
        }
    }
}
