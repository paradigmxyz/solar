#![allow(unused_crate_dependencies)]

use std::process::Command;

mod ctest;
mod soljson_js;

fn assert_command(mut command: Command, label: &str) {
    let output = command.output().unwrap_or_else(|error| panic!("failed to {label}: {error}"));
    if !output.status.success() {
        panic!(
            "failed to {label}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
