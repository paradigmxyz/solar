//! Embedders use the command-line pipeline with their own source loading.

use serde_json::Value;
use solar::{
    cli::run_compiler_with_sources,
    config::{CompileOpts, CompilerOutput, CompilerStage, Language, OptimizationMode},
    interface::{
        ColorChoice, Session, SourceMap,
        source_map::{FileLoader, FileName, RealFileLoader},
    },
};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");
const SOURCES: &[(&str, &str)] = &[
    ("Main.sol", "import './Dep.sol'; contract Main is Dep {}"),
    ("Dep.sol", "contract Dep { function answer() public pure returns (uint) { return 42; } }"),
];

#[test]
fn supplied_sources_match_cli_outputs() {
    let dir = tempfile::tempdir().unwrap();
    for &(name, contents) in SOURCES {
        fs::write(dir.path().join(name), contents).unwrap();
    }
    for (optimization, mode) in [
        (OptimizationMode::None, "none"),
        (OptimizationMode::Gas, "gas"),
        (OptimizationMode::Size, "size"),
    ] {
        let output = Command::new(SOLAR)
            .current_dir(dir.path())
            .args(["Main.sol", "--emit=abi,bin,bin-runtime", "-O", mode])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let expected = serde_json::from_slice::<Value>(&output.stdout).unwrap();
        let out_dir = dir.path().join(mode);
        fs::create_dir(&out_dir).unwrap();
        let sess = Session::builder()
            .opts(CompileOpts {
                base_path: Some(dir.path().to_owned()),
                out_dir: Some(out_dir.clone()),
                emit: vec![CompilerOutput::Abi, CompilerOutput::Bin, CompilerOutput::BinRuntime],
                optimization,
                ..Default::default()
            })
            .with_buffer_emitter(ColorChoice::Never)
            .build();
        // A valid but different disk dependency must never replace the supplied one.
        fs::write(dir.path().join("Dep.sol"), SOURCES[1].1.replace("42", "7")).unwrap();
        run_compiler_with_sources(sess, |pcx| {
            pcx.par_load_files_with_contents(
                SOURCES.iter().map(|&(name, source)| (name.to_owned(), source)).collect::<Vec<_>>(),
            )
        })
        .unwrap();
        let actual = serde_json::from_str::<Value>(
            &RealFileLoader.load_file(&out_dir.join("combined.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(actual, expected, "{mode}");
        fs::write(dir.path().join("Dep.sol"), SOURCES[1].1).unwrap();
    }
}

#[test]
fn supplied_sources_honor_stop_after_parsing() {
    let dir = tempfile::tempdir().unwrap();
    let sess = Session::builder()
        .opts(CompileOpts {
            out_dir: Some(dir.path().to_owned()),
            emit: vec![CompilerOutput::Abi],
            stop_after: Some(CompilerStage::Parsing),
            ..Default::default()
        })
        .with_buffer_emitter(ColorChoice::Never)
        .build();
    run_compiler_with_sources(sess, |pcx| {
        pcx.par_load_files_with_contents(vec![(
            "Main.sol".to_owned(),
            "contract Main is Unresolved {}",
        )])
    })
    .unwrap();
    assert!(!dir.path().join("combined.json").exists());
}

#[test]
fn supplied_source_errors_propagate() {
    for source in ["contract {", "contract Main is Unresolved {}"] {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        let result = run_compiler_with_sources(sess, |pcx| {
            pcx.par_load_files_with_contents(vec![("Main.sol".to_owned(), source)])
        });
        assert!(result.is_err(), "{source}");
    }
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    let result = run_compiler_with_sources(sess, |pcx| {
        Err(pcx.dcx().err("embedder could not load sources").emit())
    });
    assert!(result.is_err());
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    assert!(run_compiler_with_sources(sess, |_| Ok(())).is_err());
}

#[test]
fn source_callbacks_reject_other_input_interfaces() {
    for opts in [
        CompileOpts { standard_json: true, ..Default::default() },
        CompileOpts { language: Language::Mir, ..Default::default() },
        CompileOpts { language: Language::EvmIr, ..Default::default() },
    ] {
        let sess = Session::builder().opts(opts).with_buffer_emitter(ColorChoice::Never).build();
        assert!(run_compiler_with_sources(sess, |_| panic!("callback must not run")).is_err());
    }
}

#[test]
fn supplied_sources_do_not_read_host_imports() {
    let dir = tempfile::tempdir().unwrap();
    let private = dir.path().join("Private.sol");
    fs::write(&private, "contract Private {}").unwrap();
    fs::create_dir(dir.path().join("out")).unwrap();
    let path = private.to_string_lossy().replace('\\', "/");
    let source = format!("import '{path}'; contract Main is Private {{}}");
    let sess = Session::builder()
        .opts(CompileOpts {
            out_dir: Some(dir.path().join("out")),
            emit: vec![CompilerOutput::Abi],
            ..Default::default()
        })
        .with_buffer_emitter(ColorChoice::Never)
        .build();
    let source_map = sess.clone_source_map();
    let result = run_compiler_with_sources(sess, |pcx| {
        pcx.par_load_files_with_contents(vec![("Main.sol".to_owned(), source)])
    });
    assert!(result.is_err());
    assert_eq!(source_map.files().len(), 1);
}

#[test]
fn supplied_sources_reject_reused_source_maps() {
    let source_map = Arc::new(SourceMap::empty());
    for (source, success) in [("contract First {}", true), ("contract Second {}", false)] {
        let sess = Session::builder()
            .source_map(source_map.clone())
            .with_buffer_emitter(ColorChoice::Never)
            .build();
        let result = run_compiler_with_sources(sess, |pcx| {
            pcx.par_load_files_with_contents(vec![("Main.sol".to_owned(), source)])
        });
        assert_eq!(result.is_ok(), success);
    }
}

#[test]
fn supplied_sources_reject_conflicting_contents() {
    for parallel in [false, true] {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        let result = run_compiler_with_sources(sess, |pcx| {
            let files = vec![
                ("Main.sol".to_owned(), "contract First {}"),
                ("Main.sol".to_owned(), "contract Second {}"),
            ];
            if parallel {
                pcx.par_load_files_with_contents(files)
            } else {
                for file in files {
                    pcx.par_load_files_with_contents(vec![file])?;
                }
                Ok(())
            }
        });
        assert!(result.is_err());
    }
}

#[test]
fn source_callbacks_reject_command_line_inputs() {
    for input in ["ignored.sol", "ignored.yul", "ignored.mir", "ignored.evmir", "-"] {
        let sess = Session::builder()
            .opts(CompileOpts { input: vec![input.into()], ..Default::default() })
            .with_buffer_emitter(ColorChoice::Never)
            .build();
        assert!(run_compiler_with_sources(sess, |_| panic!("input must be rejected")).is_err());
    }
}

#[test]
fn supplied_sources_keep_qualified_library_addresses() {
    let dir = tempfile::tempdir().unwrap();
    let name = "library_call.sol";
    let source = "library Lib { function value() external view returns (uint) {
        return block.timestamp; } }
        contract Main { function value() external view returns (uint) { return Lib.value(); } }";
    fs::write(dir.path().join(name), source).unwrap();
    let libraries = [
        "library_call.sol:Lib=0x1111111111111111111111111111111111111111",
        "Lib=0x2222222222222222222222222222222222222222",
    ];
    let expected = Command::new(SOLAR)
        .current_dir(dir.path())
        .args([
            name,
            "--emit=abi,bin-runtime",
            "--libraries",
            libraries[0],
            "--libraries",
            libraries[1],
        ])
        .output()
        .unwrap();
    assert!(expected.status.success(), "{}", String::from_utf8_lossy(&expected.stderr));
    let expected = serde_json::from_slice::<Value>(&expected.stdout).unwrap();
    for config in ["string", "base", "current"] {
        let out_dir = dir.path().join(config);
        fs::create_dir(&out_dir).unwrap();
        let sess = Session::builder()
            .opts(CompileOpts {
                base_path: (config == "string").then(|| dir.path().to_owned()),
                out_dir: Some(out_dir.clone()),
                emit: vec![CompilerOutput::Abi, CompilerOutput::BinRuntime],
                libraries: libraries.iter().map(|library| library.parse().unwrap()).collect(),
                ..Default::default()
            })
            .with_buffer_emitter(ColorChoice::Never)
            .build();
        run_compiler_with_sources(sess, |pcx| {
            let name = if config == "string" {
                FileName::from(name.to_owned())
            } else {
                if config == "base" {
                    pcx.file_resolver.set_base_path(dir.path());
                } else {
                    pcx.file_resolver.set_current_dir(dir.path());
                }
                FileName::from(dir.path().join(name))
            };
            pcx.par_load_files_with_contents(vec![(name, source)])
        })
        .unwrap();
        let actual = serde_json::from_str::<Value>(
            &RealFileLoader.load_file(&out_dir.join("combined.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(actual, expected, "{config}");
    }
}

#[test]
fn callback_current_directory_sets_default_import_base() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Dep.sol"), SOURCES[1].1).unwrap();
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    let source_map = sess.clone_source_map();
    run_compiler_with_sources(sess, |pcx| {
        pcx.file_resolver.set_current_dir(dir.path());
        pcx.file_resolver.set_allow_file_loading(true);
        pcx.par_load_files_with_contents(vec![(PathBuf::from("Main.sol"), SOURCES[0].1)])
    })
    .unwrap();
    assert!(source_map.get_file(dir.path().join("Dep.sol")).is_some());
}

#[test]
fn supplied_sources_accept_identical_duplicates() {
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    let source_map = sess.clone_source_map();
    run_compiler_with_sources(sess, |pcx| {
        pcx.par_load_files_with_contents(vec![
            (FileName::from("Main.sol".to_owned()), "contract Main {}"),
            (FileName::real("Main.sol"), "contract Main {}"),
        ])?;
        pcx.par_load_files_with_contents(vec![("Main.sol".to_owned(), "contract Main {}")])
    })
    .unwrap();
    assert_eq!(source_map.files().len(), 1);
}

#[test]
fn supplied_sources_use_explicit_language() {
    for (language, name, source) in [
        (Language::Solidity, "Ignored.yul", "contract Main {}"),
        (Language::Yul, "Ignored.sol", "object \"Main\" { code { stop() } }"),
    ] {
        let mut opts = CompileOpts {
            language,
            stop_after: Some(CompilerStage::Parsing),
            ..Default::default()
        };
        opts.unstable.parse_yul = true;
        let sess = Session::builder().opts(opts).with_buffer_emitter(ColorChoice::Never).build();
        run_compiler_with_sources(sess, |pcx| {
            pcx.par_load_files_with_contents(vec![(name.to_owned(), source)])
        })
        .unwrap();
    }
}

#[test]
fn source_callbacks_require_explicit_loader_access() {
    struct Loader(Arc<AtomicUsize>);

    impl FileLoader for Loader {
        fn canonicalize_path(&self, path: &Path) -> io::Result<PathBuf> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(path.to_owned())
        }

        fn load_stdin(&self) -> io::Result<String> {
            panic!("stdin must not be read")
        }

        fn load_file(&self, path: &Path) -> io::Result<String> {
            self.0.fetch_add(1, Ordering::Relaxed);
            assert_eq!(path.file_name().unwrap(), "Dep.sol");
            Ok(SOURCES[1].1.into())
        }

        fn load_binary_file(&self, _path: &Path) -> io::Result<Vec<u8>> {
            panic!("binary files must not be read")
        }
    }

    for allow in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let source_map = Arc::new(SourceMap::empty());
        source_map.set_file_loader(Loader(calls.clone()));
        let sess = Session::builder()
            .source_map(source_map)
            .with_buffer_emitter(ColorChoice::Never)
            .build();
        let result = run_compiler_with_sources(sess, |pcx| {
            if allow {
                pcx.file_resolver.set_allow_file_loading(true);
            }
            pcx.par_load_files_with_contents(vec![("Main.sol".to_owned(), SOURCES[0].1)])
        });
        assert_eq!(result.is_ok(), allow);
        assert_eq!(calls.load(Ordering::Relaxed) > 0, allow);
    }
}
