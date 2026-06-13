use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn c_api_smoke_test() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_dir = manifest_dir.join("../..");
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_dir.join("target"));
    let lib_dir = target_dir.join("debug");

    let mut build = Command::new(env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo")));
    build.current_dir(&workspace_dir).args(["build", "-p", "solar-capi", "--lib"]);
    crate::assert_command(build, "build solar-capi cdylib");

    let out_dir = target_dir.join("ctest");
    fs::create_dir_all(&out_dir).unwrap();
    let exe = out_dir.join(format!("solar-capi-ctest{}", env::consts::EXE_SUFFIX));

    let Some(compiler) = c_compiler() else {
        eprintln!("skipping C API smoke test because no C compiler was found");
        return;
    };

    let source = manifest_dir.join("ctest/solidity_capi_test.c");
    let include_dir = manifest_dir.join("include");
    let runtime_lib_dir =
        dynamic_library(&lib_dir).and_then(|path| path.parent().map(Path::to_path_buf));
    let mut compile = compiler.to_command();
    if compiler.is_like_msvc() {
        let Some(import_lib) = find_existing(
            &library_search_dirs(&lib_dir),
            &["solar_capi.lib", "solar_capi.dll.lib"],
        ) else {
            panic!("failed to find solar_capi import library in {}", lib_dir.display());
        };
        compile
            .arg("/nologo")
            .arg(format!("/I{}", include_dir.display()))
            .arg(&source)
            .arg(import_lib)
            .arg(format!("/Fe:{}", exe.display()));
    } else {
        compile.arg("-I").arg(&include_dir).arg(&source);
        if cfg!(windows) {
            if let Some(import_lib) = find_existing(
                &library_search_dirs(&lib_dir),
                &["libsolar_capi.dll.a", "solar_capi.dll.a", "solar_capi.lib"],
            ) {
                compile.arg(import_lib);
            } else {
                compile.arg("-L").arg(&lib_dir).arg("-lsolar_capi");
            }
        } else {
            compile.arg("-L").arg(&lib_dir).arg("-lsolar_capi");
            compile.arg(format!("-Wl,-rpath,{}", lib_dir.display()));
        }
        compile.arg("-o").arg(&exe);
    }
    crate::assert_command(compile, "compile C API smoke test");

    let mut run = Command::new(&exe);
    prepend_dynamic_library_path(&mut run, runtime_lib_dir.as_deref().unwrap_or(&lib_dir));
    crate::assert_command(run, "run C API smoke test");
}

fn c_compiler() -> Option<cc::Tool> {
    let target = target_triple()?;
    let mut build = cc::Build::new();
    build.cargo_metadata(false).warnings(false).target(&target).host(&target).opt_level(0);
    build.try_get_compiler().ok()
}

fn target_triple() -> Option<String> {
    if let Ok(target) = env::var("TARGET") {
        return Some(target);
    }

    let output = Command::new(env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc")))
        .arg("-vV")
        .output()
        .ok()?;
    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout.lines().find_map(|line| line.strip_prefix("host: ")).map(str::to_owned)
}

fn prepend_dynamic_library_path(command: &mut Command, lib_dir: &Path) {
    let key = if cfg!(windows) {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    let mut paths = vec![lib_dir.to_path_buf()];
    if let Some(existing) = env::var_os(key) {
        paths.extend(env::split_paths(&existing));
    }
    command.env(key, env::join_paths(paths).unwrap());
}

fn dynamic_library(lib_dir: &Path) -> Option<PathBuf> {
    find_existing(&library_search_dirs(lib_dir), dynamic_library_names())
}

fn library_search_dirs(lib_dir: &Path) -> [PathBuf; 2] {
    [lib_dir.to_path_buf(), lib_dir.join("deps")]
}

fn dynamic_library_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["solar_capi.dll"]
    } else if cfg!(target_os = "macos") {
        &["libsolar_capi.dylib"]
    } else {
        &["libsolar_capi.so"]
    }
}

fn find_existing(dirs: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(OsStr::new(name))))
        .find(|path| path.exists())
}
