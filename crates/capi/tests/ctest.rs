#![allow(unused_crate_dependencies)]

use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn c_api_smoke_test() {
    if cfg!(windows) {
        eprintln!("skipping C API smoke test on Windows");
        return;
    }

    let Some(cc) = c_compiler() else {
        eprintln!("skipping C API smoke test because no C compiler was found");
        return;
    };

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_dir = manifest_dir.join("../..");
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_dir.join("target"));
    let lib_dir = target_dir.join("debug");

    let mut build = Command::new(env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo")));
    build.current_dir(&workspace_dir).args(["build", "-p", "solar-capi", "--lib"]);
    assert_command(build, "build solar-capi cdylib");

    let out_dir = target_dir.join("ctest");
    fs::create_dir_all(&out_dir).unwrap();
    let exe = out_dir.join("solar-capi-ctest");

    let mut compile = Command::new(cc);
    compile
        .arg("-I")
        .arg(manifest_dir.join("include"))
        .arg(manifest_dir.join("ctest/solidity_capi_test.c"))
        .arg("-L")
        .arg(&lib_dir)
        .arg("-lsolar_capi")
        .arg(format!("-Wl,-rpath,{}", lib_dir.display()))
        .arg("-o")
        .arg(&exe);
    assert_command(compile, "compile C API smoke test");

    let mut run = Command::new(&exe);
    prepend_dynamic_library_path(&mut run, &lib_dir);
    assert_command(run, "run C API smoke test");
}

fn c_compiler() -> Option<OsString> {
    if let Some(cc) = env::var_os("CC") {
        return Some(cc);
    }
    ["cc", "clang", "gcc"].into_iter().find(|cc| command_exists(cc)).map(OsString::from)
}

fn command_exists(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

fn prepend_dynamic_library_path(command: &mut Command, lib_dir: &Path) {
    let key = if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" };
    let mut paths = vec![lib_dir.to_path_buf()];
    if let Some(existing) = env::var_os(key) {
        paths.extend(env::split_paths(&existing));
    }
    command.env(key, env::join_paths(paths).unwrap());
}

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
