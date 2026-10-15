//! Embedders use the command-line pipeline with their own source loading.

use serde_json::Value;
use solar::{
    cli::run_compiler_with_sources,
    config::{CompileOpts, CompilerOutput, CompilerStage, Language, OptimizationMode},
    interface::{
        ColorChoice, Session,
        source_map::{FileLoader, RealFileLoader},
    },
};
use std::{fs, process::Command};

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
                // The callback supplies every input; this path must never be read.
                input: vec!["does-not-exist.sol".into()],
                base_path: Some(dir.path().to_owned()),
                out_dir: Some(out_dir.clone()),
                emit: vec![CompilerOutput::Abi, CompilerOutput::Bin, CompilerOutput::BinRuntime],
                optimization,
                ..Default::default()
            })
            .with_buffer_emitter(ColorChoice::Never)
            .build();
        // The dependency is supplied in memory, with different contents from the disk file.
        fs::write(dir.path().join("Dep.sol"), "not Solidity").unwrap();
        run_compiler_with_sources(sess, |pcx| {
            pcx.par_load_files_with_contents(
                SOURCES
                    .iter()
                    .map(|&(name, source)| (dir.path().join(name), source))
                    .collect::<Vec<_>>(),
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
