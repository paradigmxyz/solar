//! Wall-clock samples for requests waiting on analysis after a document edit.

#![allow(unused_crate_dependencies)]

use lsp_types::{
    DidChangeTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams,
    Position, Range, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentPositionParams, Url, VersionedTextDocumentIdentifier,
};
use serde_json::{Value, json};
use solar_config::CompileOpts;
use solar_interface::source_map::SourceMap;
use solar_lsp::{BenchmarkPendingRequests, BenchmarkProject, BenchmarkRequest, BenchmarkResponse};
use std::{fmt::Write as _, fs, path::PathBuf};

struct Expected {
    position: Position,
    hover: Option<Hover>,
    definition: Option<GotoDefinitionResponse>,
}

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    uri: Url,
    source: String,
    expected: [Expected; 2],
}

impl Fixture {
    fn new(unifap: bool) -> Self {
        let directory = tempfile::tempdir().expect("pending benchmark directory");
        let source_map = SourceMap::empty();
        let file_loader = source_map.file_loader();
        let root = file_loader.canonicalize_path(directory.path()).unwrap();
        let mut sources = Vec::new();
        let (path, anchor) = if unifap {
            let original =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/foundry/unifap-v2/src");
            for relative in [
                "UnifapV2Router.sol",
                "libraries/UnifapV2Library.sol",
                "interfaces/IUnifapV2Factory.sol",
                "interfaces/IUnifapV2Pair.sol",
                "interfaces/IERC20.sol",
            ] {
                let contents = file_loader.load_file(&original.join(relative)).unwrap();
                let destination = root.join("lib/unifap").join(relative);
                fs::create_dir_all(destination.parent().unwrap()).unwrap();
                fs::write(&destination, &contents).unwrap();
                sources.push((destination, contents));
            }
            let source = "import \"./lib/unifap/UnifapV2Router.sol\";\n".to_owned();
            fs::write(root.join("Main.sol"), &source).unwrap();
            sources.push((root.join("Main.sol"), source));
            (
                root.join("lib/unifap/UnifapV2Router.sol"),
                "_safeTransferFrom(address(pair), msg.sender, address(pair), liquidity)",
            )
        } else {
            let mut source = String::from(
                "contract Main {\n    function target(uint256 value) internal pure returns (uint256) { return value; }\n\n",
            );
            for index in 0..255 {
                writeln!(
                    source,
                    "    function function_{index:04}() public pure returns (uint256) {{ return target({index}); }}\n"
                )
                .unwrap();
            }
            source.push_str("}\n");
            fs::write(root.join("Main.sol"), &source).unwrap();
            sources.push((root.join("Main.sol"), source));
            (root.join("Main.sol"), "target(0)")
        };
        let uri = Url::from_file_path(&path).unwrap();
        let source = sources.iter().find(|(candidate, _)| candidate == &path).unwrap().1.clone();
        let [(before, before_expected), (after, after_expected)] = std::array::from_fn(|shift| {
            let mut inputs = sources.clone();
            if shift == 1 {
                inputs
                    .iter_mut()
                    .find(|(candidate, _)| candidate == &path)
                    .unwrap()
                    .1
                    .insert(0, '\n');
            }
            let project = BenchmarkProject::from_sources(
                CompileOpts { base_path: Some(root.clone()), ..Default::default() },
                inputs,
            )
            .unwrap();
            let (uri, position) =
                project.unique_anchor(path.strip_prefix(&root).unwrap(), anchor).unwrap();
            let analysis = project.analyze();
            assert_eq!(analysis.diagnostic_count(), 0, "{}", analysis.diagnostic_fingerprint());
            let BenchmarkResponse::Hover(hover) =
                analysis.execute(&BenchmarkRequest::Hover { uri: uri.clone(), position })
            else {
                unreachable!();
            };
            let BenchmarkResponse::GotoDefinition(definition) =
                analysis.execute(&BenchmarkRequest::GotoDefinition { uri, position })
            else {
                unreachable!();
            };
            assert!(hover.is_some() && definition.is_some());
            (analysis, Expected { position, hover, definition })
        });
        let expected = [before_expected, after_expected];
        // A stale table at the edited cursor must not accidentally resolve an adjacent reference.
        for (stale, fresh) in [(&before, &expected[1]), (&after, &expected[0])] {
            let BenchmarkResponse::Hover(hover) = stale
                .execute(&BenchmarkRequest::Hover { uri: uri.clone(), position: fresh.position })
            else {
                unreachable!();
            };
            let BenchmarkResponse::GotoDefinition(definition) =
                stale.execute(&BenchmarkRequest::GotoDefinition {
                    uri: uri.clone(),
                    position: fresh.position,
                })
            else {
                unreachable!();
            };
            assert_ne!(hover, fresh.hover, "stale hover must fail preflight");
            assert_ne!(definition, fresh.definition, "stale definition must fail preflight");
        }
        Self { _directory: directory, root, uri, source, expected }
    }

    async fn measure(&self, method: &str, warmup_count: usize, sample_count: usize) -> Value {
        let mut requests =
            BenchmarkPendingRequests::new(self.root.clone(), self.uri.clone(), self.source.clone())
                .await;
        let source_change_debounce_ms = requests.source_change_debounce().as_millis();
        let mut samples_ns = Vec::with_capacity(sample_count);
        for index in 0..warmup_count + sample_count {
            let shift = (index + 1) % 2;
            let expected = &self.expected[shift];
            let change = DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(
                    self.uri.clone(),
                    i32::try_from(index + 2).expect("benchmark document version overflow"),
                ),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: Some(Range::new(
                        Position::new(0, 0),
                        Position::new(u32::from(shift == 0), 0),
                    )),
                    range_length: None,
                    text: if shift == 1 { "\n".into() } else { String::new() },
                }],
            };
            let position = TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(self.uri.clone()),
                position: expected.position,
            };
            let elapsed = match method {
                "hover" => {
                    let (elapsed, response) = requests
                        .hover(
                            change,
                            HoverParams {
                                text_document_position_params: position,
                                work_done_progress_params: Default::default(),
                            },
                        )
                        .await;
                    assert_eq!(response, expected.hover);
                    elapsed
                }
                "definition" => {
                    let (elapsed, response) = requests
                        .definition(
                            change,
                            GotoDefinitionParams {
                                text_document_position_params: position,
                                work_done_progress_params: Default::default(),
                                partial_result_params: Default::default(),
                            },
                        )
                        .await;
                    assert_eq!(response, expected.definition);
                    elapsed
                }
                _ => unreachable!(),
            };
            if index >= warmup_count {
                samples_ns.push(u64::try_from(elapsed.as_nanos()).unwrap());
            }
        }
        let mut sorted = samples_ns.clone();
        sorted.sort_unstable();
        let percentile = |percent: usize| sorted[(sorted.len() * percent).div_ceil(100) - 1];
        json!({
            "method": method,
            "source_change_debounce_ms": source_change_debounce_ms,
            "samples_ns": samples_ns,
            "p50_ns": percentile(50),
            "p95_ns": percentile(95),
        })
    }
}

fn count(name: &str, default: usize) -> usize {
    match std::env::var(name) {
        Ok(value) => {
            value.parse().unwrap_or_else(|_| panic!("{name} must be a non-negative integer"))
        }
        Err(std::env::VarError::NotPresent) => default,
        Err(error) => panic!("invalid {name}: {error}"),
    }
}

fn main() {
    let sample_count = count("SOLAR_LSP_BENCH_SAMPLES", 30);
    let warmup_count = count("SOLAR_LSP_BENCH_WARMUP", 5);
    assert!(sample_count > 0, "SOLAR_LSP_BENCH_SAMPLES must be positive");
    assert!(
        warmup_count.checked_add(sample_count).is_some_and(|count| count < i32::MAX as usize),
        "too many benchmark iterations"
    );
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut results = Vec::new();
    for (corpus, unifap) in [("generated-256-functions", false), ("unifap-v2-import", true)] {
        let fixture = Fixture::new(unifap);
        for method in ["hover", "definition"] {
            let mut result = runtime.block_on(fixture.measure(method, warmup_count, sample_count));
            result["corpus"] = json!(corpus);
            results.push(result);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "boundary": "in-process-didChange-to-response",
            "compiler_threads": 1,
            "target_os": std::env::consts::OS,
            "target_arch": std::env::consts::ARCH,
            "warmup_count": warmup_count,
            "sample_count": sample_count,
            "percentile_method": "nearest-rank",
            "results": results,
        }))
        .unwrap()
    );
}
