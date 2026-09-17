//! Name every case `lsp/<operation>[<scenario>]` using parameter IDs so CodSpeed retains the
//! operation. Include the corpus or workload size with units, plus any cache or cursor state.

#![allow(unused_crate_dependencies)]

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use crop::Rope;
use lsp_types::{
    GotoDefinitionResponse, HoverContents, OneOf, Position, Range, TextDocumentContentChangeEvent,
    Url,
};
use solar_config::CompileOpts;
use solar_lsp::{
    BenchmarkAnalysis, BenchmarkCallHierarchyRequests, BenchmarkDocumentChange,
    BenchmarkDocumentUpdate, BenchmarkFoldingRangeRequests, BenchmarkOpenDocuments,
    BenchmarkProject, BenchmarkRenameRequests, BenchmarkRepeatedAnalysis, BenchmarkRequest,
    BenchmarkResponse, BenchmarkSelectionRangeRequests, BenchmarkSignatureHelpRequests,
    BenchmarkWorkspaceDiscovery, BenchmarkWorkspacePathQueries, BenchmarkWorkspaceReports,
    benchmark_folding_ranges, benchmark_folding_ranges_from_rope, benchmark_import_path_at,
    benchmark_selection_ranges,
};
use solar_parse::{Cursor, lexer::token::RawTokenKind};
use std::{fmt::Write as _, fs, hint::black_box, path::PathBuf};

const ANALYSIS_FUNCTION_COUNTS: [usize; 2] = [64, 256];
const INCOMPLETE_FOLDING_CONTRACT_COUNT: usize = 256;
const MINIFIED_FOLDING_FUNCTION_COUNT: usize = 1_024;
const HOVER_FUNCTION_COUNT: usize = 256;
const AGGREGATION_BATCH_COUNT: usize = 4;
const AGGREGATION_FUNCTION_COUNT: usize = 64;
const PATH_INDEX_QUERY_COUNT: usize = 1024;
const PATH_INDEX_WORKSPACE_COUNT: usize = 16;
const OPEN_DOCUMENT_COUNT: usize = 16;
const OPEN_DOCUMENT_BYTES: usize = 256 * 1024;
const UNIFAP_PROJECT: &str = "unifap-v2";
const UNIFAP_ROUTER: &str = "src/UnifapV2Router.sol";
const UNIFAP_PAIR: &str = "src/UnifapV2Pair.sol";
const UNIFAP_FACTORY: &str = "src/UnifapV2Factory.sol";
const OPTIMISM_SOURCE: &str = include_str!("../../../testdata/Optimism.sol");

struct BenchmarkSource {
    source: String,
    project: BenchmarkProject,
    hover_positions: Vec<(u32, u32)>,
}

fn benchmark_source(function_count: usize) -> BenchmarkSource {
    let mut source = String::new();
    let mut hover_anchors = Vec::with_capacity(function_count * 2);
    let mut push_line = |line: &str| {
        source.push_str(line);
        source.push('\n');
    };
    push_line("contract Benchmark {");
    for index in 0..function_count {
        let name = format!("function_{index:04}");
        push_line(&format!("    /// @notice Processes values for benchmark function {index}."));
        push_line("    /// @dev Used to measure resolved NatSpec rendering.");
        push_line("    /// @param first The first input value.");
        push_line("    /// @param second The second input value.");
        push_line("    /// @param account The account returned by the function.");
        push_line("    /// @return total The sum of both input values.");
        push_line("    /// @return owner The supplied account.");
        let declaration = format!(
            "    function {name}(uint256 first, uint256 second, address account) public pure returns (uint256 total, address owner) {{"
        );
        push_line(&declaration);
        hover_anchors.push(format!("{name}(uint256 first"));
        push_line("        total = first + second;");
        push_line("        owner = account;");
        push_line("    }");
    }

    push_line("    function exercise() public pure {");
    for index in 0..function_count {
        let name = format!("function_{index:04}");
        let call = format!("        {name}(1, 2, address(0));");
        push_line(&call);
        hover_anchors.push(format!("{name}(1, 2, address(0))"));
    }
    push_line("    }");
    push_line("}");
    let project = BenchmarkProject::from_source(source.clone());
    let hover_positions = hover_anchors
        .into_iter()
        .map(|anchor| {
            let (_, position) = project
                .unique_anchor("benchmark.sol", &anchor)
                .expect("generated hover anchors should be unique");
            (position.line, position.character)
        })
        .collect();
    BenchmarkSource { source, project, hover_positions }
}

fn analysis_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/analysis-build");
    for function_count in ANALYSIS_FUNCTION_COUNTS {
        let fixture = benchmark_source(function_count);
        let analysis = BenchmarkAnalysis::from_source(fixture.source.clone());
        assert_clean(&analysis);
        group.throughput(Throughput::Bytes(fixture.source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{function_count}-functions")),
            &fixture.source,
            |b, source| {
                b.iter_batched(
                    || source.clone(),
                    |source| black_box(BenchmarkAnalysis::from_source(black_box(source))),
                    BatchSize::PerIteration,
                );
            },
        );
    }
    let call = "function_0000(1, 2, address(0));";
    let source = benchmark_source(1).source.replace(call, &call.repeat(256));
    assert_clean(&BenchmarkAnalysis::from_source(source.clone()));
    group.throughput(Throughput::Bytes(source.len() as u64));
    group.bench_function(BenchmarkId::from_parameter("1-function-256-repeated-calls"), |b| {
        b.iter_batched(
            || source.clone(),
            |source| black_box(BenchmarkAnalysis::from_source(black_box(source))),
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

fn call_hierarchy_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/call-hierarchy");
    for caller_count in [128, 2_048] {
        let mut source = String::from("contract Root { function target() internal {}\n");
        for index in 0..caller_count {
            writeln!(source, "function caller{index}() public {{ target(); }}").unwrap();
        }
        source.push_str("}\n");
        let project = BenchmarkProject::from_source(source);
        let (uri, target_position) =
            project.unique_anchor("benchmark.sol", "target() internal").unwrap();
        let analysis = project.clone().analyze();
        assert_clean(&analysis);
        assert_eq!(analysis.incoming_calls(&uri, target_position).len(), caller_count);
        group.bench_function(
            BenchmarkId::from_parameter(format!("{caller_count}-callers-incoming")),
            |b| {
                b.iter(|| {
                    black_box(analysis.incoming_calls(black_box(&uri), black_box(target_position)))
                });
            },
        );

        let caller_line = format!("function caller{}() public {{ target(); }}", caller_count - 1);
        let (body_uri, line_start) = project.unique_anchor("benchmark.sol", &caller_line).unwrap();
        let body_position =
            Position::new(line_start.line, line_start.character + "function ".len() as u32);
        assert_eq!(analysis.prepare_call_hierarchy(&body_uri, body_position).unwrap().len(), 1);
        group.bench_function(
            BenchmarkId::from_parameter(format!("{caller_count}-callers-prepare-body")),
            |b| {
                b.iter(|| {
                    black_box(
                        analysis
                            .prepare_call_hierarchy(black_box(&body_uri), black_box(body_position)),
                    )
                })
            },
        );

        let call_position = Position::new(
            line_start.line,
            line_start.character
                + format!("function caller{}() public {{ ", caller_count - 1).len() as u32,
        );
        assert_eq!(analysis.prepare_call_hierarchy(&body_uri, call_position).unwrap().len(), 1);
        group.bench_function(
            BenchmarkId::from_parameter(format!("{caller_count}-callers-prepare-callsite")),
            |b| {
                b.iter(|| {
                    black_box(
                        analysis
                            .prepare_call_hierarchy(black_box(&body_uri), black_box(call_position)),
                    )
                })
            },
        );

        group.bench_function(
            BenchmarkId::from_parameter(format!(
                "{caller_count}-callers-prepare-body-first-request"
            )),
            |b| {
                b.iter_batched(
                    || analysis.clone(),
                    |cold| black_box(cold.prepare_call_hierarchy(&body_uri, call_position)),
                    BatchSize::PerIteration,
                )
            },
        );
    }
    group.finish();
}

fn call_hierarchy_requests(c: &mut Criterion) {
    let project = unifap_project();
    let analysis = project.clone().analyze();
    assert_clean(&analysis);
    let mut requests = BenchmarkCallHierarchyRequests::new(analysis);
    let (uri, mut declaration) =
        project.unique_anchor(UNIFAP_ROUTER, "function _safeTransferFrom(").unwrap();
    declaration.character += "function ".len() as u32;
    let (_, body) = project.unique_anchor(UNIFAP_ROUTER, "success = IERC20(token)").unwrap();
    let (_, callsite) = project
        .unique_anchor(UNIFAP_ROUTER, "_safeTransferFrom(tokenA, msg.sender, pair, amountA)")
        .unwrap();
    let prepared = requests.prepare(&uri, declaration).unwrap();
    assert_eq!(prepared.len(), 1);
    let helper = &prepared[0];
    assert_eq!(helper.name, "_safeTransferFrom");
    assert_eq!(helper.detail.as_deref(), Some("UnifapV2Router"));
    assert_eq!(helper.uri, uri);
    assert_eq!(
        helper.selection_range,
        Range::new(
            declaration,
            Position::new(declaration.line, declaration.character + helper.name.len() as u32),
        ),
    );
    let positions = [("declaration", declaration), ("body", body), ("callsite", callsite)];
    for (_, position) in positions {
        assert_eq!(requests.prepare(&uri, position), Some(prepared.clone()));
        assert_eq!(requests.before_first_request().prepare(&uri, position), Some(prepared.clone()));
    }

    let call_range = |call: &str, name: &str| {
        let (_, mut start) = project.unique_anchor(UNIFAP_ROUTER, call).unwrap();
        start.character += call.find(name).unwrap() as u32;
        Range::new(start, Position::new(start.line, start.character + name.len() as u32))
    };
    let incoming = requests.incoming(helper).unwrap();
    assert_eq!(incoming.len(), 2);
    for (call, name, expected_ranges) in [
        (
            &incoming[0],
            "addLiquidity",
            vec![
                call_range("_safeTransferFrom(tokenA, msg.sender, pair, amountA)", &helper.name),
                call_range("_safeTransferFrom(tokenB, msg.sender, pair, amountB)", &helper.name),
            ],
        ),
        (
            &incoming[1],
            "removeLiquidity",
            vec![call_range(
                "_safeTransferFrom(address(pair), msg.sender, address(pair), liquidity)",
                &helper.name,
            )],
        ),
    ] {
        let (_, mut position) =
            project.unique_anchor(UNIFAP_ROUTER, &format!("function {name}(")).unwrap();
        position.character += "function ".len() as u32;
        assert_eq!(requests.prepare(&uri, position), Some(vec![call.from.clone()]));
        assert_eq!(call.from.name, name);
        assert_eq!(call.from_ranges, expected_ranges);
    }
    let outgoing = requests.outgoing(helper).unwrap();
    assert_eq!(outgoing.len(), 1);
    let (token_uri, mut token_position) =
        project.unique_anchor("src/interfaces/IERC20.sol", "function transferFrom(").unwrap();
    token_position.character += "function ".len() as u32;
    assert_eq!(requests.prepare(&token_uri, token_position), Some(vec![outgoing[0].to.clone()]));
    assert_eq!(outgoing[0].to.name, "transferFrom");
    assert_eq!(
        outgoing[0].from_ranges,
        [call_range("IERC20(token).transferFrom(from, to, amount)", "transferFrom")],
    );
    for (caller, expected_names, range_count) in [
        (
            &incoming[0],
            &["check", "_computeLiquidityAmounts", "_safeTransferFrom", "pairs", "mint"][..],
            6,
        ),
        (&incoming[1], &["check", "_safeTransferFrom", "pairs", "burn", "sortPairs"][..], 5),
    ] {
        let expanded = requests.outgoing(&caller.from).unwrap();
        assert_eq!(
            expanded.iter().map(|call| call.to.name.as_str()).collect::<Vec<_>>(),
            expected_names
        );
        assert_eq!(expanded.iter().map(|call| call.from_ranges.len()).sum::<usize>(), range_count);
        assert_eq!(requests.before_first_request().outgoing(&caller.from), Some(expanded));
    }
    assert_eq!(requests.before_first_request().incoming(helper), Some(incoming));
    assert_eq!(requests.before_first_request().outgoing(helper), Some(outgoing));

    let mut group = c.benchmark_group("lsp/call-hierarchy-request");
    for (location, position) in positions {
        group.bench_function(
            BenchmarkId::from_parameter(format!("unifap-v2-prepare-{location}")),
            |b| b.iter(|| black_box(requests.prepare(black_box(&uri), black_box(position)))),
        );
    }
    group.bench_function(BenchmarkId::from_parameter("unifap-v2-incoming"), |b| {
        b.iter(|| black_box(requests.incoming(black_box(helper))));
    });
    group.bench_function(BenchmarkId::from_parameter("unifap-v2-outgoing"), |b| {
        b.iter(|| black_box(requests.outgoing(black_box(helper))));
    });
    group.finish();

    let mut group = c.benchmark_group("lsp/call-hierarchy-expand");
    group.throughput(Throughput::Elements(5));
    group.bench_function(BenchmarkId::from_parameter("unifap-v2-transfer-helper"), |b| {
        b.iter(|| {
            let items = requests.prepare(black_box(&uri), black_box(callsite)).unwrap();
            let callers = requests.incoming(black_box(&items[0])).unwrap();
            for caller in &callers {
                black_box(requests.outgoing(black_box(&caller.from)));
            }
            black_box(requests.outgoing(black_box(&items[0])));
        });
    });
    group.finish();

    let mut group = c.benchmark_group("lsp/call-hierarchy-first-request");
    group.bench_function(BenchmarkId::from_parameter("unifap-v2-router"), |b| {
        b.iter_batched_ref(
            || requests.before_first_request(),
            |requests| black_box(requests.prepare(black_box(&uri), black_box(callsite))),
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

fn rename_candidate_queries(c: &mut Criterion) {
    let mut source = String::from("contract Root { function target() internal {}\n");
    for index in 0..2_048 {
        writeln!(source, "function caller{index}() public {{ target(); }}").unwrap();
    }
    source.push_str("}\n");
    let project = BenchmarkProject::from_source(source);
    let (uri, eof_anchor) = project
        .unique_anchor("benchmark.sol", "function caller2047() public { target(); }")
        .unwrap();
    let hit_position = Position::new(
        eof_anchor.line,
        eof_anchor.character + "function caller2047() public { ".len() as u32,
    );
    let analysis = project.analyze();
    assert_clean(&analysis);
    let Some((range, edit_count)) = analysis.rename_candidate(&uri, hit_position) else {
        panic!("rename candidate should resolve at the final call site");
    };
    assert_eq!(edit_count, 2_049);
    assert!(range.start <= hit_position && hit_position < range.end);
    let miss_position = Position::new(eof_anchor.line + 1, 0);
    assert!(analysis.rename_candidate(&uri, miss_position).is_none());

    let mut group = c.benchmark_group("lsp/rename-candidate");
    group.bench_function(BenchmarkId::from_parameter("2048-callers-hit-near-eof"), |b| {
        b.iter(|| black_box(analysis.rename_candidate(black_box(&uri), black_box(hit_position))))
    });
    group.bench_function(BenchmarkId::from_parameter("2048-callers-miss-near-eof"), |b| {
        b.iter(|| black_box(analysis.rename_candidate(black_box(&uri), black_box(miss_position))))
    });
    group.finish();
}

fn rename_requests(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/rename");
    for reference_count in [0, 64, 2_048] {
        let mut source = String::from("contract Root { function target() internal pure {}\n");
        for index in 0..reference_count {
            writeln!(source, "function caller{index}() public pure {{ target(); }}").unwrap();
        }
        source.push_str("}\n");
        let project = BenchmarkProject::from_source(source);
        let (uri, position) = project.unique_anchor("benchmark.sol", "target() internal").unwrap();
        let mut requests = BenchmarkRenameRequests::new(project, uri.clone(), position);
        let response = requests.run().expect("the target should be renameable");
        let edits = &response.changes.as_ref().unwrap()[&uri];
        assert_eq!(edits.len(), reference_count + 1);
        assert!(edits.iter().all(|edit| edit.new_text == "renamed"));
        group.bench_function(
            BenchmarkId::from_parameter(format!("{reference_count}-references")),
            |b| {
                b.iter(|| black_box(requests.run()));
            },
        );
    }

    let project = unifap_project();
    let (uri, position) = project.unique_anchor(UNIFAP_ROUTER, "_safeTransferFrom(\n").unwrap();
    let mut requests = BenchmarkRenameRequests::new(project, uri, position);
    let response = requests.run().expect("the router helper should be renameable");
    let edits = response.changes.unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits.values().next().unwrap().len(), 4);
    group.bench_function(BenchmarkId::from_parameter("unifap-v2-router"), |b| {
        b.iter(|| black_box(requests.run()));
    });
    group.finish();
}

fn type_hierarchy_queries(c: &mut Criterion) {
    let mut source = String::from("contract Root {}\n");
    for index in 0..128 {
        writeln!(source, "contract Child{index} is Root {{}}").unwrap();
    }
    let project = BenchmarkProject::from_source(source);
    let (uri, position) =
        project.unique_anchor("benchmark.sol", "Root {}\ncontract Child0").unwrap();
    let analysis = project.analyze();
    assert_clean(&analysis);
    assert_eq!(analysis.type_hierarchy(&uri, position).len(), 128);
    let mut group = c.benchmark_group("lsp/type-hierarchy");
    group.bench_function(BenchmarkId::from_parameter("128-subtypes"), |b| {
        b.iter(|| black_box(analysis.type_hierarchy(black_box(&uri), black_box(position))));
    });
    group.finish();
}

fn code_lens_queries(c: &mut Criterion) {
    let fixture = benchmark_source(HOVER_FUNCTION_COUNT);
    let (uri, _) =
        fixture.project.unique_anchor("benchmark.sol", "function_0255(1, 2, address(0))").unwrap();
    let analysis = fixture.project.analyze();
    assert_clean(&analysis);
    let mut first_requests = vec![("256-functions".to_owned(), analysis.clone(), uri.clone())];
    assert!(analysis.code_lenses(&uri).len() >= HOVER_FUNCTION_COUNT);
    let mut group = c.benchmark_group("lsp/code-lens");
    group.bench_function(BenchmarkId::from_parameter("256-functions"), |b| {
        b.iter(|| black_box(analysis.code_lenses(black_box(&uri))));
    });

    for reference_count in [64, 1_024, 16_384] {
        let mut source = String::from(
            "contract RepeatedReferences {\nfunction target() internal pure {}\nfunction exercise() public pure {\n",
        );
        for _ in 0..reference_count {
            source.push_str("target();\n");
        }
        source.push_str("}\n}\n");
        let project = BenchmarkProject::from_source(source);
        let (uri, position) = project.unique_anchor("benchmark.sol", "target() internal").unwrap();
        let analysis = project.analyze();
        assert_clean(&analysis);
        first_requests.push((
            format!("{reference_count}-references"),
            analysis.clone(),
            uri.clone(),
        ));
        let lenses = analysis.code_lenses(&uri);
        assert_eq!(lenses.len(), 4);
        assert!(lenses.iter().any(|lens| {
            lens.range.start == position
                && lens.command.as_ref().unwrap().title == format!("{reference_count} references")
        }));
        group.bench_function(
            BenchmarkId::from_parameter(format!("{reference_count}-references")),
            |b| b.iter(|| black_box(analysis.code_lenses(black_box(&uri)))),
        );
    }

    let project = unifap_project();
    let (uri, _) = project.unique_anchor(UNIFAP_PAIR, "SELECTOR").unwrap();
    let analysis = project.analyze();
    assert_clean(&analysis);
    first_requests.push(("unifap-v2-pair".to_owned(), analysis.clone(), uri.clone()));
    assert!(!analysis.code_lenses(&uri).is_empty());
    group.bench_function(BenchmarkId::from_parameter("unifap-v2-pair"), |b| {
        b.iter(|| black_box(analysis.code_lenses(black_box(&uri))));
    });
    group.finish();

    let mut group = c.benchmark_group("lsp/code-lens-first-request");
    for (name, analysis, uri) in first_requests {
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter_batched_ref(
                || analysis.clone(),
                |analysis| black_box(analysis.code_lenses(black_box(&uri))),
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn document_symbol_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/document-symbol");
    for function_count in [256, 1_024] {
        let fixture = benchmark_source(function_count);
        let (uri, _) = fixture
            .project
            .unique_anchor("benchmark.sol", "contract Benchmark")
            .expect("the document-symbol anchor should be unique");
        let analysis = fixture.project.analyze();
        assert_clean(&analysis);
        let symbols = analysis.document_symbols(&uri);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].children.as_ref().map_or(0, Vec::len), function_count + 1);
        group.throughput(Throughput::Elements(function_count as u64));
        group.bench_function(
            BenchmarkId::from_parameter(format!("{function_count}-functions")),
            |b| {
                b.iter(|| black_box(analysis.document_symbols(black_box(&uri))));
            },
        );
    }
    group.finish();
}

fn import_path_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/import-path");
    let cursor = OPTIMISM_SOURCE.rfind('}').unwrap();
    assert!(!benchmark_import_path_at(OPTIMISM_SOURCE, cursor));
    group.bench_function(BenchmarkId::from_parameter("optimism-non-import-at-end"), |b| {
        b.iter(|| {
            black_box(benchmark_import_path_at(black_box(OPTIMISM_SOURCE), black_box(cursor)))
        });
    });
    group.finish();
}

fn completion_queries(c: &mut Criterion) {
    let fixture = benchmark_source(HOVER_FUNCTION_COUNT);
    let (uri, position) =
        fixture.project.unique_anchor("benchmark.sol", "function_0255(1, 2, address(0))").unwrap();
    let analysis = fixture.project.analyze();
    assert_clean(&analysis);
    let mut group = c.benchmark_group("lsp/completion");
    for (name, prefix) in [
        ("all", ""),
        ("selective", "function_0255"),
        ("fuzzy", "f0255"),
        ("no-match", "not_a_symbol"),
        ("long-no-match", "function_0255_extra"),
    ] {
        let items = analysis.completions(&uri, position, prefix);
        match name {
            "all" => assert!(items.len() >= HOVER_FUNCTION_COUNT),
            "selective" | "fuzzy" => assert_eq!(
                items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(),
                ["function_0255"]
            ),
            _ => assert!(items.is_empty()),
        }
        group.bench_function(
            BenchmarkId::from_parameter(format!("{HOVER_FUNCTION_COUNT}-functions-{name}")),
            |b| {
                b.iter(|| {
                    black_box(analysis.completions(
                        black_box(&uri),
                        black_box(position),
                        black_box(prefix),
                    ))
                });
            },
        );
    }
    group.finish();
}

fn member_completion_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/member-completion");
    for access_count in [64, 256, 1024] {
        let mut source = String::from(
            "contract Benchmark {\nstruct Value { uint256 field; }\nfunction exercise(Value memory value) public pure returns (uint256 result) {\n",
        );
        for index in 0..access_count {
            writeln!(source, "    result += value.field; // {index}").unwrap();
        }
        source.push_str("}\n}\n");
        let project = BenchmarkProject::from_source(source);
        let anchor = format!("value.field; // {}", access_count - 1);
        let (uri, mut position) = project.unique_anchor("benchmark.sol", &anchor).unwrap();
        position.character += "value.field".len() as u32;
        let analysis = project.analyze();
        assert_clean(&analysis);
        let items = analysis.completions(&uri, position, "field");
        assert_eq!(items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(), ["field"]);
        group.bench_function(
            BenchmarkId::from_parameter(format!("{access_count}-member-accesses")),
            |b| {
                b.iter(|| {
                    black_box(analysis.completions(
                        black_box(&uri),
                        black_box(position),
                        black_box("field"),
                    ))
                });
            },
        );
    }
    group.finish();
}

fn signature_help_requests(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/signature-help");
    for function_count in [64, 256, 1024] {
        let fixture = benchmark_source(function_count);
        let anchor = format!("function_{:04}(1, ", function_count - 1);
        let (uri, mut position) = fixture.project.unique_anchor("benchmark.sol", &anchor).unwrap();
        position.character += anchor.len() as u32;
        let mut requests = BenchmarkSignatureHelpRequests::new(fixture.project, uri, position);
        let response = requests.run().expect("benchmark call should have signature help");
        assert_eq!(response.active_parameter, Some(1));
        assert_eq!(response.signatures.len(), 1);
        assert!(
            response.signatures[0].label.contains(&format!("function_{:04}", function_count - 1))
        );
        group.bench_function(
            BenchmarkId::from_parameter(format!("{function_count}-functions")),
            |b| {
                b.iter(|| black_box(requests.run()));
            },
        );
    }

    let mut source = String::from(
        "contract Repeated { function target(uint256 first, uint256 second) public {} function exercise() public {\n",
    );
    for _ in 0..1_023 {
        source.push_str("target(1, 2);\n");
    }
    source.push_str("target(1, 2); // final\n}\n}\n");
    let project = BenchmarkProject::from_source(source);
    let (uri, mut position) =
        project.unique_anchor("benchmark.sol", "target(1, 2); // final").unwrap();
    position.character += "target(1, ".len() as u32;
    let mut requests = BenchmarkSignatureHelpRequests::new(project, uri, position);
    let response = requests.run().expect("repeated-call benchmark should have signature help");
    assert_eq!(response.active_parameter, Some(1));
    group.bench_function(BenchmarkId::from_parameter("1024-repeated-calls"), |b| {
        b.iter(|| black_box(requests.run()));
    });

    let project = unifap_project();
    let (uri, mut position) = project
        .unique_anchor(UNIFAP_ROUTER, "_safeTransferFrom(tokenB, msg.sender, pair, amountB)")
        .unwrap();
    position.character += "_safeTransferFrom(tokenB, ".len() as u32;
    let mut requests = BenchmarkSignatureHelpRequests::new(project, uri, position);
    let response = requests.run().expect("router call should have signature help");
    assert_eq!(response.active_parameter, Some(1));
    assert_eq!(response.signatures.len(), 1);
    assert!(response.signatures[0].label.starts_with("function _safeTransferFrom("));
    group.bench_function(BenchmarkId::from_parameter("unifap-v2-router"), |b| {
        b.iter(|| black_box(requests.run()));
    });
    assert_eq!(requests.after_edit().run(), Some(response));
    group.bench_function(BenchmarkId::from_parameter("unifap-v2-router-after-edit"), |b| {
        b.iter_batched_ref(
            || requests.after_edit(),
            |requests| black_box(requests.run()),
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

fn signature_help_moving_cursors(c: &mut Criterion) {
    let mut workloads = Vec::new();
    for function_count in [64, 256, 1024] {
        let source = benchmark_source(function_count).source.replacen(
            "contract Benchmark {\n",
            "contract Benchmark {\nconstructor() { function_0000(3, 4, address(0)); }\n",
            1,
        );
        let project = BenchmarkProject::from_source(source);
        let signature = |index| {
            format!(
                "function function_{index:04}(uint256 first, uint256 second, address account) public pure returns (uint256 total, address owner)"
            )
        };
        let (uri, mut early) =
            project.unique_anchor("benchmark.sol", "function_0000(3, 4, address(0))").unwrap();
        early.character += "function_0000(".len() as u32;
        let mut positions = Vec::new();
        for index in function_count - 8..function_count {
            let callee = format!("function_{index:04}(");
            let (_, start) = project
                .unique_anchor("benchmark.sol", &format!("{callee}1, 2, address(0))"))
                .unwrap();
            for (parameter, prefix) in ["", "1, ", "1, 2, "].into_iter().enumerate() {
                let position = Position::new(
                    start.line,
                    start.character + (callee.len() + prefix.len()) as u32,
                );
                positions.push((position, parameter as u32, signature(index)));
            }
        }
        let late = positions.last().unwrap().clone();
        let requests = BenchmarkSignatureHelpRequests::new(project, uri, early);
        workloads.push((
            format!("{function_count}-functions"),
            requests,
            positions,
            (early, 0, signature(0)),
            late,
        ));
    }

    let project = unifap_project();
    let mut positions = Vec::new();
    for (call, arguments, label) in [
        (
            "_safeTransferFrom(tokenA, msg.sender, pair, amountA)",
            &["tokenA", "msg.sender", "pair", "amountA"][..],
            "function _safeTransferFrom(address token, address from, address to, uint256 amount) internal returns (bool success)",
        ),
        (
            "_safeTransferFrom(tokenB, msg.sender, pair, amountB)",
            &["tokenB", "msg.sender", "pair", "amountB"][..],
            "function _safeTransferFrom(address token, address from, address to, uint256 amount) internal returns (bool success)",
        ),
        (
            "UnifapV2Library.sortPairs(tokenA, tokenB)",
            &["tokenA", "tokenB"][..],
            "function sortPairs(address token0, address token1) internal pure returns (address, address)",
        ),
        (
            "UnifapV2Library.quote(amountADesired, reserveA, reserveB)",
            &["amountADesired", "reserveA", "reserveB"][..],
            "function quote(uint256 amount0, uint256 reserve0, uint256 reserve1) internal pure returns (uint256)",
        ),
        (
            "IERC20(token).transferFrom(from, to, amount)",
            &["from", "to,", "amount"][..],
            "function transferFrom(address from, address to, uint256 amount) external returns (bool)",
        ),
    ] {
        let (_, start) = project.unique_anchor(UNIFAP_ROUTER, call).unwrap();
        for (parameter, argument) in arguments.iter().enumerate() {
            let position =
                Position::new(start.line, start.character + call.find(argument).unwrap() as u32);
            positions.push((position, parameter as u32, label.to_owned()));
        }
    }
    let early = positions.first().unwrap().clone();
    let late = positions.last().unwrap().clone();
    let (uri, _) = project
        .unique_anchor(UNIFAP_ROUTER, "_safeTransferFrom(tokenA, msg.sender, pair, amountA)")
        .unwrap();
    let requests = BenchmarkSignatureHelpRequests::new(project, uri, early.0);
    workloads.push(("unifap-v2-router".into(), requests, positions, early, late));

    for (_, requests, positions, early, late) in &mut workloads {
        for (position, parameter, label) in positions.iter().chain([&*early, &*late]) {
            let response = requests.run_at(*position).expect("moving cursor should resolve a call");
            assert_eq!(response.active_signature, Some(0));
            assert_eq!(response.active_parameter, Some(*parameter));
            assert_eq!(response.signatures.len(), 1);
            assert_eq!(&response.signatures[0].label, label);
            assert_eq!(requests.before_first_request().run_at(*position), Some(response.clone()));
            assert_eq!(requests.after_edit().run_at(*position), Some(response));
        }
    }

    let mut group = c.benchmark_group("lsp/signature-help-moving-cursor");
    for (name, requests, positions, _, _) in &mut workloads {
        group.throughput(Throughput::Elements(positions.len() as u64));
        group.bench_function(
            BenchmarkId::from_parameter(format!("{name}-{}-cursor-positions", positions.len())),
            |b| {
                b.iter(|| {
                    for (position, _, _) in black_box(&*positions) {
                        black_box(requests.run_at(black_box(*position)));
                    }
                });
            },
        );
    }
    group.finish();

    for edited in [false, true] {
        let mut group = c.benchmark_group(if edited {
            "lsp/signature-help-first-after-edit"
        } else {
            "lsp/signature-help-first-request"
        });
        for (name, requests, _, early, late) in &workloads {
            for (location, &(position, _, _)) in [("early", early), ("late", late)] {
                group.bench_function(
                    BenchmarkId::from_parameter(format!("{name}-{location}-cursor")),
                    |b| {
                        b.iter_batched_ref(
                            || {
                                if edited {
                                    requests.after_edit()
                                } else {
                                    requests.before_first_request()
                                }
                            },
                            |requests| black_box(requests.run_at(black_box(position))),
                            BatchSize::PerIteration,
                        );
                    },
                );
            }
        }
        group.finish();
    }
}

fn bounded_workspace_discovery(c: &mut Criterion) {
    let temp = tempfile::tempdir().expect("benchmark temporary directory");
    let project = temp.path().join("project");
    let dependency = project.join("vendor");
    fs::create_dir_all(&dependency).expect("dependency directory");
    for index in 0..10_000 {
        fs::write(dependency.join(format!("Generated{index}.sol")), "contract Generated {} {}")
            .expect("dependency source");
    }
    fs::write(project.join("foundry.toml"), "[profile.default]\nlibs = [\"vendor\"]\n")
        .expect("Foundry project manifest");

    let baseline = BenchmarkWorkspaceDiscovery::run(temp.path());
    assert_eq!(baseline.eager(), 0);
    assert_eq!(baseline.source_file_count(), 0);
    // Manifest and source discovery each prune the import-only root without visiting its
    // 10,000 descendants. Loading the workspace scans it once for remappings.
    assert_eq!(baseline.pruned(), 2);
    // Include the project root and configured entry-point roots, even when absent.
    assert_eq!(baseline.visited(), 9);

    let mut group = c.benchmark_group("lsp/workspace-discovery");
    group.bench_function(BenchmarkId::from_parameter("foundry-10000-import-only-files"), |b| {
        b.iter(|| black_box(BenchmarkWorkspaceDiscovery::run(black_box(temp.path()))));
    });
    group.finish();
}

fn aggregation_project() -> BenchmarkProject {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/aggregation");
    let source = benchmark_source(AGGREGATION_FUNCTION_COUNT).source;
    let sources = (0..AGGREGATION_BATCH_COUNT)
        .map(|index| (PathBuf::from(format!("batch-{index}.sol")), source.clone()));
    let opts = CompileOpts { base_path: Some(root), ..Default::default() };
    BenchmarkProject::from_sources(opts, sources)
        .expect("the aggregation benchmark project should be valid")
}

fn symbol_table_aggregation(c: &mut Criterion) {
    let project = aggregation_project();
    let batches = project.clone().analyze_file_batches();
    assert_eq!(batches.len(), AGGREGATION_BATCH_COUNT);
    assert!(batches.iter().all(|batch| batch.diagnostic_count() == 0));

    let query = format!("function_{:04}", AGGREGATION_FUNCTION_COUNT - 1);
    let mut group = c.benchmark_group("lsp/symbol-table-aggregation");
    for batch_count in [1, AGGREGATION_BATCH_COUNT] {
        let merged = BenchmarkAnalysis::merge(batches[..batch_count].to_vec());
        assert_clean(&merged);
        let response = merged.execute(&BenchmarkRequest::WorkspaceSymbols { query: query.clone() });
        let BenchmarkResponse::WorkspaceSymbols(symbols) = response else {
            panic!("the aggregation check should return workspace symbols")
        };
        let mut uris = symbols
            .into_iter()
            .map(|symbol| {
                let OneOf::Left(location) = symbol.location else {
                    panic!("workspace symbols should contain full locations")
                };
                location.uri
            })
            .collect::<Vec<_>>();
        uris.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
        let expected_uris = (0..batch_count)
            .map(|index| {
                Url::from_file_path(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("benches/aggregation")
                        .join(format!("batch-{index}.sol")),
                )
                .expect("the aggregation benchmark path should be a file URI")
            })
            .collect::<Vec<_>>();
        assert_eq!(uris, expected_uris);

        group.throughput(Throughput::Elements(batch_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!(
                "{batch_count}-batches-{AGGREGATION_FUNCTION_COUNT}-functions-per-batch"
            )),
            &batch_count,
            |b, &batch_count| {
                b.iter_batched(
                    || batches[..batch_count].to_vec(),
                    |batches| black_box(BenchmarkAnalysis::merge(black_box(batches))),
                    BatchSize::PerIteration,
                );
            },
        );
    }
    group.finish();

    let mut group = c.benchmark_group("lsp/project-analysis-batched");
    group.bench_function(
        BenchmarkId::from_parameter(format!(
            "{AGGREGATION_BATCH_COUNT}-batches-{AGGREGATION_FUNCTION_COUNT}-functions-per-batch"
        )),
        |b| {
            b.iter_batched(
                || project.clone(),
                |project| {
                    black_box(BenchmarkAnalysis::merge(black_box(project.analyze_file_batches())))
                },
                BatchSize::PerIteration,
            );
        },
    );
    group.finish();
}

fn burst_hover(c: &mut Criterion) {
    let fixture = benchmark_source(HOVER_FUNCTION_COUNT);
    let analysis = fixture.project.analyze();
    let positions = fixture.hover_positions;
    assert_clean(&analysis);
    assert_eq!(positions.len(), HOVER_FUNCTION_COUNT * 2);
    assert!(positions.iter().all(|&(line, character)| analysis.hover(line, character).is_some()));

    let mut group = c.benchmark_group("lsp/burst-hover");
    group.throughput(Throughput::Elements(positions.len() as u64));
    group.bench_function(
        BenchmarkId::from_parameter(format!(
            "{HOVER_FUNCTION_COUNT}-functions-{}-requests",
            positions.len()
        )),
        |b| {
            b.iter(|| {
                let analysis = black_box(&analysis);
                for &(line, character) in black_box(&positions) {
                    black_box(analysis.hover(black_box(line), black_box(character)));
                }
            });
        },
    );
    group.finish();
}

fn selection_range(c: &mut Criterion) {
    let positions = [Position::new(0, 0)];
    let ranges = benchmark_selection_ranges(OPTIMISM_SOURCE.to_owned(), &positions)
        .expect("the benchmark position should be valid");
    assert_eq!(ranges.len(), positions.len());
    assert!(ranges[0].range.start <= positions[0] && positions[0] < ranges[0].range.end);

    let mut group = c.benchmark_group("lsp/selection-range");
    group.throughput(Throughput::Bytes(OPTIMISM_SOURCE.len() as u64));
    group.bench_function(BenchmarkId::from_parameter("optimism-start-1-position"), |b| {
        b.iter_batched(
            || OPTIMISM_SOURCE.to_owned(),
            |source| {
                black_box(benchmark_selection_ranges(black_box(source), black_box(&positions)))
            },
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

fn incomplete_folding_source(contract_count: usize) -> String {
    let mut source = String::new();
    for prefix in ["before", "after"] {
        if prefix == "after" {
            source.push_str("@ incomplete edit\n");
        }
        for index in 0..contract_count {
            source.push_str(&format!(
                "contract {prefix}_{index:04} {{\n    function f() external {{\n        if (true) {{\n        }}\n    }}\n}}\n"
            ));
            if prefix == "after" && index % 16 == 15 {
                source.push_str("@ incomplete edit\n");
            }
        }
    }
    source
}

fn minified_folding_source(function_count: usize) -> String {
    let mut source = String::from("contract Minified{");
    for index in 0..function_count {
        source.push_str(&format!("function f{index}() external{{if(true){{}}}}"));
    }
    source.push('}');
    source
}

fn rope_to_string(rope: &Rope) -> String {
    let mut source = String::with_capacity(rope.byte_len());
    for chunk in rope.chunks() {
        source.push_str(chunk);
    }
    source
}

fn folding_range(c: &mut Criterion) {
    let clean = OPTIMISM_SOURCE.to_owned();
    let incomplete = incomplete_folding_source(INCOMPLETE_FOLDING_CONTRACT_COUNT);
    let minified = minified_folding_source(MINIFIED_FOLDING_FUNCTION_COUNT);
    let open_rope = Rope::from(clean.as_str());

    let clean_ranges = benchmark_folding_ranges(clean.clone());
    let incomplete_ranges = benchmark_folding_ranges(incomplete.clone());
    let minified_ranges = benchmark_folding_ranges(minified.clone());
    assert!(!clean_ranges.is_empty());
    assert_eq!(incomplete_ranges.len(), INCOMPLETE_FOLDING_CONTRACT_COUNT * 2 * 3);
    assert!(minified_ranges.is_empty());
    assert_eq!(benchmark_folding_ranges_from_rope(open_rope.clone()), clean_ranges);

    let mut group = c.benchmark_group("lsp/folding-range");
    for (name, source) in [
        ("optimism-clean".to_owned(), clean),
        (format!("{}-contracts-incomplete", INCOMPLETE_FOLDING_CONTRACT_COUNT * 2), incomplete),
        (format!("{MINIFIED_FOLDING_FUNCTION_COUNT}-functions-minified"), minified),
    ] {
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &source, |b, source| {
            b.iter_batched(
                || source.clone(),
                |source| black_box(benchmark_folding_ranges(black_box(source))),
                BatchSize::PerIteration,
            );
        });
    }
    group.throughput(Throughput::Bytes(OPTIMISM_SOURCE.len() as u64));
    group.bench_function(
        BenchmarkId::from_parameter("optimism-open-document-rope-to-string"),
        |b| {
            b.iter_batched(
                || open_rope.clone(),
                |rope| {
                    let source = rope_to_string(&rope);
                    black_box(benchmark_folding_ranges(black_box(source)))
                },
                BatchSize::PerIteration,
            );
        },
    );
    group.bench_function(
        BenchmarkId::from_parameter("optimism-open-document-rope-snapshot"),
        |b| {
            b.iter_batched(
                || open_rope.clone(),
                |rope| black_box(benchmark_folding_ranges_from_rope(black_box(rope))),
                BatchSize::PerIteration,
            );
        },
    );
    group.finish();

    let requests = BenchmarkFoldingRangeRequests::new(OPTIMISM_SOURCE.to_owned());
    assert_eq!(requests.run(), clean_ranges);
    let mut cached = c.benchmark_group("lsp/open-document-folding-range");
    cached.throughput(Throughput::Bytes(OPTIMISM_SOURCE.len() as u64));
    cached.bench_function(BenchmarkId::from_parameter("optimism-unchanged"), |b| {
        b.iter(|| black_box(requests.run()));
    });
    cached.bench_function(BenchmarkId::from_parameter("optimism-first-request"), |b| {
        b.iter_batched_ref(
            || BenchmarkFoldingRangeRequests::new(OPTIMISM_SOURCE.to_owned()),
            |requests| black_box(requests.run()),
            BatchSize::PerIteration,
        );
    });
    cached.finish();
}

fn open_document_selection_range(c: &mut Criterion) {
    let position_at = |source: &str, offset| {
        let prefix = &source[..offset];
        Position::new(
            prefix.bytes().filter(|&byte| byte == b'\n').count() as u32,
            prefix.rsplit('\n').next().unwrap().encode_utf16().count() as u32,
        )
    };
    let middle = position_at(
        OPTIMISM_SOURCE,
        OPTIMISM_SOURCE
            .match_indices("return ")
            .find(|(offset, _)| *offset >= OPTIMISM_SOURCE.len() / 2)
            .unwrap()
            .0,
    );
    let end = position_at(OPTIMISM_SOURCE, OPTIMISM_SOURCE.rfind("return ").unwrap());
    let start = Position::new(0, 0);

    let mut group = c.benchmark_group("lsp/open-document-selection-range");
    group.throughput(Throughput::Bytes(OPTIMISM_SOURCE.len() as u64));
    for (name, positions) in [
        ("optimism-start-1-position", vec![start]),
        ("optimism-middle-1-position", vec![middle]),
        ("optimism-end-1-position", vec![end]),
        ("optimism-mixed-4-positions", vec![end, start, middle, end]),
    ] {
        let requests = BenchmarkSelectionRangeRequests::new(
            OPTIMISM_SOURCE.to_owned(),
            positions.iter().copied(),
        );
        let expected = benchmark_selection_ranges(OPTIMISM_SOURCE.to_owned(), &positions)
            .expect("the benchmark positions should be valid");
        let ranges = requests.run().expect("the benchmark positions should be valid");
        assert_eq!(ranges, expected);
        assert_eq!(ranges.len(), positions.len());
        for (range, position) in ranges.iter().zip(&positions) {
            assert!(range.range.start <= *position && *position < range.range.end);
            if *position != start {
                assert!(range.parent.is_some());
            }
        }
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| black_box(black_box(&requests).run()));
        });
    }
    for (name, source) in [
        ("uniswap-v3", include_str!("../../../testdata/UniswapV3.sol")),
        (
            "unifap-v2-router",
            include_str!("../../../tests/foundry/unifap-v2/src/UnifapV2Router.sol"),
        ),
    ] {
        let anchor = "\n    function ";
        let offset = source
            .match_indices(anchor)
            .find(|(offset, _)| *offset >= source.len() / 2)
            .expect("the real source should contain a function declaration")
            .0
            + anchor.len();
        let positions = [position_at(source, offset)];
        let requests = BenchmarkSelectionRangeRequests::new(source.to_owned(), positions);
        let expected = benchmark_selection_ranges(source.to_owned(), &positions)
            .expect("the benchmark position should be valid");
        assert!(expected[0].parent.is_some());
        assert_eq!(requests.run(), Some(expected));
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| black_box(black_box(&requests).run()));
        });
    }
    group.finish();

    let mut lines = c.benchmark_group("lsp/open-document-selection-range-line-layout");
    for (name, separator, comment) in [
        ("minified-ascii", " ", ""),
        ("minified-unicode", " ", "/* 😀 */"),
        ("multiline", "\n", ""),
    ] {
        let mut source = String::from("contract LineLayout {");
        for index in 0..1024 {
            write!(
                source,
                "{separator}{comment}function f{index}() external pure returns(uint){{return 123456;}}"
            )
            .unwrap();
        }
        source.push('}');
        let literal_start = source.rfind("123456").unwrap();
        let positions = [position_at(&source, literal_start + 2)];
        let expected = benchmark_selection_ranges(source.clone(), &positions)
            .expect("the benchmark position should be valid");
        assert_eq!(
            expected[0].range,
            lsp_types::Range::new(
                position_at(&source, literal_start),
                position_at(&source, literal_start + "123456".len()),
            )
        );
        assert!(expected[0].parent.is_some());
        lines.throughput(Throughput::Bytes(source.len() as u64));
        let requests = BenchmarkSelectionRangeRequests::new(source, positions);
        assert_eq!(requests.run(), Some(expected));
        lines.bench_function(
            BenchmarkId::from_parameter(format!("1024-functions-{name}-1-position")),
            |b| {
                b.iter(|| black_box(black_box(&requests).run()));
            },
        );
    }
    lines.finish();

    let mut cold = c.benchmark_group("lsp/open-document-selection-range-cold");
    cold.throughput(Throughput::Bytes(OPTIMISM_SOURCE.len() as u64));
    cold.bench_function(BenchmarkId::from_parameter("optimism-middle-1-position"), |b| {
        b.iter_batched_ref(
            || BenchmarkSelectionRangeRequests::new(OPTIMISM_SOURCE.to_owned(), [middle]),
            |requests| black_box(requests.run()),
            BatchSize::PerIteration,
        );
    });
    cold.finish();
}

fn workspace_diagnostic_hot_paths(c: &mut Criterion) {
    let source = OPTIMISM_SOURCE.to_owned();
    assert_eq!(BenchmarkDocumentUpdate::from_source(source.clone()).apply(), 1);
    let mut updates = c.benchmark_group("lsp/unchanged-document-update");
    updates.throughput(Throughput::Bytes(source.len() as u64));
    updates.bench_function(BenchmarkId::from_parameter("optimism"), |b| {
        b.iter_batched(
            || BenchmarkDocumentUpdate::from_source(source.clone()),
            |update| black_box(update.apply()),
            BatchSize::PerIteration,
        );
    });
    updates.finish();

    let mut reports = c.benchmark_group("lsp/workspace-diagnostic-reports");
    for document_count in [256, 4096] {
        assert_eq!(
            BenchmarkWorkspaceReports::new(document_count).generate(),
            document_count + document_count / 4
        );
        reports.throughput(Throughput::Elements(document_count as u64));
        reports.bench_with_input(
            BenchmarkId::from_parameter(format!("{document_count}-documents")),
            &document_count,
            |b, &document_count| {
                b.iter_batched(
                    || BenchmarkWorkspaceReports::new(document_count),
                    |reports| black_box(reports.generate()),
                    BatchSize::PerIteration,
                );
            },
        );
    }
    reports.finish();
}

fn incoming_document_changes(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/incoming-document-changes");
    for (name, source, identifier, occurrence_count, edit_counts) in [
        ("optimism-predeploys", OPTIMISM_SOURCE, "Predeploys", 377, &[1, 8, 64, 377][..]),
        (
            "uniswap-tickmath",
            include_str!("../../../testdata/UniswapV3.sol"),
            "TickMath",
            20,
            &[1, 20][..],
        ),
        (
            "counter",
            "contract Counter {\n    uint256 count;\n    function increment() public { count++; }\n    function value() public view returns (uint256) { return count; }\n}\n",
            "count",
            3,
            &[1, 3][..],
        ),
    ] {
        let occurrences = Cursor::new(source)
            .with_position()
            .filter_map(|(start, token)| {
                let end = start + token.len as usize;
                (token.kind == RawTokenKind::Ident && &source[start..end] == identifier)
                    .then_some(start..end)
            })
            .collect::<Vec<_>>();
        assert_eq!(occurrences.len(), occurrence_count);
        let position_at = |offset| {
            let prefix = &source[..offset];
            Position::new(
                prefix.bytes().filter(|&byte| byte == b'\n').count() as u32,
                prefix.rsplit('\n').next().unwrap().encode_utf16().count() as u32,
            )
        };
        let contents = Rope::from(source);
        for &edit_count in edit_counts {
            let replacement = format!("{identifier}Renamed");
            let mut expected = source.to_owned();
            let mut changes = Vec::with_capacity(edit_count);
            // Clients apply independent replacements from the end to preserve earlier positions.
            for range in occurrences.iter().rev().take(edit_count) {
                changes.push(TextDocumentContentChangeEvent {
                    range: Some(Range::new(position_at(range.start), position_at(range.end))),
                    range_length: None,
                    text: replacement.clone(),
                });
                expected.replace_range(range.clone(), &replacement);
            }
            let change = BenchmarkDocumentChange::from_changes(contents.clone(), changes);
            assert_eq!(change.clone().apply().contents().to_string(), expected);
            group.throughput(Throughput::Elements(edit_count as u64));
            group.bench_function(
                BenchmarkId::from_parameter(format!("{name}-{edit_count}-edits")),
                |b| {
                    b.iter_batched(
                        || change.clone(),
                        |change| black_box(change.apply()),
                        BatchSize::PerIteration,
                    );
                },
            );
        }
    }

    // Overlapping ranges must retain the sequential LSP behavior and exercise the fallback path.
    let source = Rope::from("abcdef\nghijkl\n");
    let changes = vec![
        TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 2), Position::new(0, 4))),
            range_length: None,
            text: "X".into(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 1), Position::new(0, 3))),
            range_length: None,
            text: "Y".into(),
        },
    ];
    let change = BenchmarkDocumentChange::from_changes(source, changes);
    assert_eq!(change.clone().apply().contents().to_string(), "aYef\nghijkl\n");
    group.throughput(Throughput::Elements(2));
    group.bench_function(
        BenchmarkId::from_parameter("2-overlapping-edits-sequential-fallback"),
        |b| {
            b.iter_batched(
                || change.clone(),
                |change| black_box(change.apply()),
                BatchSize::PerIteration,
            );
        },
    );
    group.finish();
}

fn open_document_analysis_batches(c: &mut Criterion) {
    let documents = BenchmarkOpenDocuments::new(OPEN_DOCUMENT_COUNT, OPEN_DOCUMENT_BYTES);
    // Prime the initial snapshot so timing measures reuse across later analysis epochs.
    assert!(documents.build_analysis_batches() >= documents.source_bytes());

    let mut group = c.benchmark_group("lsp/open-document-analysis-batches");
    group.throughput(Throughput::Bytes(documents.source_bytes() as u64));
    group.bench_function(
        BenchmarkId::from_parameter(format!(
            "{OPEN_DOCUMENT_COUNT}-documents-{OPEN_DOCUMENT_BYTES}-bytes-per-document"
        )),
        |b| b.iter(|| black_box(documents.build_analysis_batches())),
    );
    group.finish();
}

fn repeated_analysis(c: &mut Criterion) {
    let fixture = benchmark_source(256);

    let mut cold = c.benchmark_group("lsp/incremental-analysis");
    cold.bench_function(BenchmarkId::from_parameter("256-functions-cold"), |b| {
        b.iter_batched(
            || BenchmarkRepeatedAnalysis::new(fixture.source.clone()),
            |mut analysis| black_box(analysis.run()),
            BatchSize::PerIteration,
        );
    });
    cold.finish();

    let mut analysis = BenchmarkRepeatedAnalysis::new(fixture.source);
    assert!(analysis.run());
    let mut cached = c.benchmark_group("lsp/incremental-analysis");
    cached.bench_function(BenchmarkId::from_parameter("256-functions-unchanged"), |b| {
        b.iter(|| black_box(analysis.run()))
    });
    cached.bench_function(BenchmarkId::from_parameter("256-functions-reverted-edit"), |b| {
        b.iter(|| {
            analysis.edit_and_revert();
            black_box(analysis.run())
        });
    });
    cached.finish();
}

fn single_workspace_index_reuse(c: &mut Criterion) {
    let temp = tempfile::tempdir().expect("single workspace benchmark directory");
    let root = temp.path().to_path_buf();
    fs::create_dir(root.join("lib")).unwrap();
    let mut generated = String::from(
        "import \"./lib/Dependency.sol\";\ncontract Main is Dependency {\nfunction target() internal {}\n",
    );
    for index in 0..256 {
        writeln!(generated, "function caller{index}() public {{ target(); }}").unwrap();
    }
    generated.push_str("}\n");
    fs::write(root.join("lib/Dependency.sol"), "contract Dependency {}\n").unwrap();

    let real_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/foundry/unifap-v2/src");
    fn copy_sources(source: &std::path::Path, destination: &std::path::Path) {
        fs::create_dir_all(destination).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let destination = destination.join(entry.file_name());
            if path.is_dir() {
                copy_sources(&path, &destination);
            } else if path.extension().is_some_and(|extension| extension == "sol") {
                fs::copy(path, destination).unwrap();
            }
        }
    }
    copy_sources(&real_root, &root.join("lib/unifap"));
    let imported = "import \"./lib/unifap/UnifapV2Router.sol\";\n";
    let main = root.join("Main.sol");
    let uri = Url::from_file_path(&main).unwrap();
    for (name, source) in [("256-callers", generated.as_str()), ("unifap-v2-import", imported)] {
        fs::write(&main, source).unwrap();
        let prepare =
            || BenchmarkRepeatedAnalysis::from_workspaces(std::slice::from_ref(&root), source);
        let mut analysis = prepare();
        assert!(analysis.run_epoch());
        analysis.assert_no_diagnostics();
        if name == "256-callers" {
            assert_eq!(
                analysis.prepare_call_hierarchy(&uri, Position::new(3, 9)).unwrap().len(),
                1
            );
        } else {
            let router = Url::from_file_path(root.join("lib/unifap/UnifapV2Router.sol")).unwrap();
            let (_, mut position) = unifap_project()
                .unique_anchor(UNIFAP_ROUTER, "function _safeTransferFrom(")
                .unwrap();
            position.character += "function ".len() as u32;
            assert_eq!(
                analysis.prepare_call_hierarchy(&router, position).unwrap()[0].name,
                "_safeTransferFrom"
            );
        }
        c.benchmark_group("lsp/single-workspace-unchanged").bench_function(
            BenchmarkId::from_parameter(name),
            |b| {
                b.iter(|| black_box(analysis.run_epoch()));
            },
        );
        c.benchmark_group("lsp/single-workspace-cold").bench_function(
            BenchmarkId::from_parameter(name),
            |b| {
                b.iter_batched_ref(
                    prepare,
                    |analysis| black_box(analysis.run_epoch()),
                    BatchSize::PerIteration,
                );
            },
        );
        c.benchmark_group("lsp/single-workspace-reverted-edit").bench_function(
            BenchmarkId::from_parameter(name),
            |b| {
                b.iter(|| {
                    analysis.edit_and_revert();
                    black_box(analysis.run_epoch())
                });
            },
        );
        c.benchmark_group("lsp/single-workspace-open-indexed").bench_function(
            BenchmarkId::from_parameter(name),
            |b| {
                b.iter_batched_ref(
                    || {
                        let mut analysis = prepare();
                        analysis.clear_open_documents();
                        assert!(analysis.run_epoch());
                        analysis.assert_no_diagnostics();
                        analysis
                    },
                    |analysis| {
                        analysis.replace_source(&main, source);
                        black_box(analysis.run_epoch())
                    },
                    BatchSize::PerIteration,
                );
            },
        );
        let mut edited = false;
        let edited_source = format!("{source} ");
        c.benchmark_group("lsp/single-workspace-changed").bench_function(
            BenchmarkId::from_parameter(name),
            |b| {
                b.iter(|| {
                    edited = !edited;
                    analysis.replace_source(&main, if edited { &edited_source } else { source });
                    black_box(analysis.run_epoch())
                });
            },
        );
    }
}

fn workspace_index_reuse(c: &mut Criterion) {
    let workspace_count = 4;
    let caller_count = 256;
    let temp = tempfile::tempdir().expect("workspace index benchmark directory");
    let mut source = String::from(
        "import \"./lib/Dependency.sol\";\ncontract Main is Dependency {\nfunction target() internal {}\n",
    );
    for index in 0..caller_count {
        writeln!(source, "function caller{index}() public {{ target(); }}").unwrap();
    }
    source.push_str("uint marker0;\n}\n");
    let edited_source = source.replace("marker0", "marker1");
    let roots = (0..workspace_count)
        .map(|index| {
            let root = temp.path().join(format!("workspace-{index}"));
            fs::create_dir(&root).expect("benchmark workspace root");
            fs::create_dir(root.join("lib")).expect("benchmark dependency directory");
            fs::write(root.join("Main.sol"), &source).expect("benchmark source");
            fs::write(root.join("lib/Dependency.sol"), "contract Dependency {}\n")
                .expect("benchmark dependency");
            root
        })
        .collect::<Vec<_>>();
    let path = roots[0].join("Main.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let position = Position::new(
        caller_count as u32 + 2,
        format!("function caller{}() public {{ ", caller_count - 1).len() as u32,
    );

    let mut group = c.benchmark_group("lsp/workspace-index-reuse");
    group.bench_function(
        BenchmarkId::from_parameter("4-workspaces-256-callers-per-workspace-cold"),
        |b| {
            b.iter_batched_ref(
                || BenchmarkRepeatedAnalysis::from_workspaces(&roots, &source),
                |analysis| black_box(analysis.run_epoch()),
                BatchSize::PerIteration,
            );
        },
    );
    group.bench_function(
        BenchmarkId::from_parameter(
            "4-workspaces-256-callers-per-workspace-open-indexed-document-first-call-hierarchy",
        ),
        |b| {
            b.iter_batched_ref(
                || {
                    let mut analysis = BenchmarkRepeatedAnalysis::from_workspaces(&roots, &source);
                    analysis.clear_open_documents();
                    assert!(analysis.run_epoch());
                    assert_eq!(analysis.prepare_call_hierarchy(&uri, position).unwrap().len(), 1);
                    analysis
                },
                |analysis| {
                    analysis.replace_source(&path, &source);
                    black_box(analysis.run_epoch());
                    black_box(analysis.prepare_call_hierarchy(&uri, position))
                },
                BatchSize::PerIteration,
            );
        },
    );

    let mut analysis = BenchmarkRepeatedAnalysis::from_workspaces(&roots, &source);
    assert!(analysis.run_epoch());
    assert_eq!(analysis.prepare_call_hierarchy(&uri, position).unwrap().len(), 1);
    // Initialization and the first lazy query happen once, outside every incremental sample.
    group.bench_function(
        BenchmarkId::from_parameter("4-workspaces-256-callers-per-workspace-unchanged"),
        |b| {
            b.iter(|| black_box(analysis.run_epoch()));
        },
    );
    group.bench_function(
        BenchmarkId::from_parameter(
            "4-workspaces-256-callers-per-workspace-unchanged-first-call-hierarchy",
        ),
        |b| {
            b.iter(|| {
                black_box(analysis.run_epoch());
                black_box(analysis.prepare_call_hierarchy(&uri, position))
            });
        },
    );
    group.bench_function(
        BenchmarkId::from_parameter(
            "4-workspaces-256-callers-per-workspace-reverted-edit-first-call-hierarchy",
        ),
        |b| {
            b.iter(|| {
                analysis.edit_and_revert();
                black_box(analysis.run_epoch());
                black_box(analysis.prepare_call_hierarchy(&uri, position))
            });
        },
    );
    let mut edited = false;
    group.bench_function(
        BenchmarkId::from_parameter(
            "4-workspaces-256-callers-per-workspace-one-workspace-edit-first-call-hierarchy",
        ),
        |b| {
            b.iter(|| {
                edited = !edited;
                analysis.replace_source(&path, if edited { &edited_source } else { &source });
                black_box(analysis.run_epoch());
                black_box(analysis.prepare_call_hierarchy(&uri, position))
            });
        },
    );
    group.finish();
}

fn workspace_path_queries(c: &mut Criterion) {
    let queries =
        BenchmarkWorkspacePathQueries::new(PATH_INDEX_WORKSPACE_COUNT, PATH_INDEX_QUERY_COUNT);
    assert_ne!(queries.run(), 0);
    assert_eq!(queries.run(), queries.run_cached());

    let mut group = c.benchmark_group("lsp/workspace-path-queries");
    group.throughput(Throughput::Elements(PATH_INDEX_QUERY_COUNT as u64));
    group.bench_function(
        BenchmarkId::from_parameter(format!(
            "{PATH_INDEX_WORKSPACE_COUNT}-workspaces-{PATH_INDEX_QUERY_COUNT}-queries"
        )),
        |b| {
            b.iter(|| black_box(queries.run()));
        },
    );
    group.bench_function(
        BenchmarkId::from_parameter(format!(
            "{PATH_INDEX_WORKSPACE_COUNT}-workspaces-{PATH_INDEX_QUERY_COUNT}-queries-cached"
        )),
        |b| {
            b.iter(|| black_box(queries.run_cached()));
        },
    );
    group.finish();

    let mut group = c.benchmark_group("lsp/workspace-path-single-query");
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        BenchmarkId::from_parameter(format!(
            "{PATH_INDEX_WORKSPACE_COUNT}-workspaces-single-query"
        )),
        |b| {
            b.iter(|| black_box(queries.run_one()));
        },
    );
    group.bench_function(
        BenchmarkId::from_parameter(format!(
            "{PATH_INDEX_WORKSPACE_COUNT}-workspaces-single-query-cached"
        )),
        |b| {
            b.iter(|| black_box(queries.run_cached_one()));
        },
    );
    group.finish();

    let mut group = c.benchmark_group("lsp/workspace-path-containment-query");
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        BenchmarkId::from_parameter(format!(
            "{PATH_INDEX_WORKSPACE_COUNT}-workspaces-containment-query"
        )),
        |b| {
            b.iter(|| black_box(queries.run_containment_one()));
        },
    );
    group.finish();
}

fn unifap_project() -> BenchmarkProject {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/foundry/unifap-v2/foundry.toml");
    let project = BenchmarkProject::from_foundry_manifest(manifest)
        .expect("the tracked unifap-v2 benchmark project should load");
    assert_eq!(project.file_count(), 14);
    project
}

fn unifap_requests(project: &BenchmarkProject) -> [(&'static str, BenchmarkRequest); 4] {
    let (hover_uri, hover_position) =
        project.unique_anchor(UNIFAP_PAIR, "SELECTOR").expect("the hover anchor should be unique");
    let (definition_uri, definition_position) = project
        .unique_anchor(UNIFAP_ROUTER, "sortPairs")
        .expect("the definition anchor should be unique");
    let (references_uri, references_position) = project
        .unique_anchor(UNIFAP_FACTORY, "getAllPairLength")
        .expect("the references anchor should be unique");

    [
        ("hover", BenchmarkRequest::Hover { uri: hover_uri, position: hover_position }),
        (
            "goto-definition",
            BenchmarkRequest::GotoDefinition { uri: definition_uri, position: definition_position },
        ),
        (
            "references",
            BenchmarkRequest::References {
                uri: references_uri,
                position: references_position,
                include_declaration: true,
            },
        ),
        ("workspace-symbols", BenchmarkRequest::WorkspaceSymbols { query: "UnifapV2Pair".into() }),
    ]
}

fn assert_unifap_response(name: &str, response: BenchmarkResponse) {
    match (name, response) {
        ("hover", BenchmarkResponse::Hover(Some(hover))) => {
            let HoverContents::Markup(markup) = hover.contents else {
                panic!("the SELECTOR hover should contain markup")
            };
            assert!(markup.value.contains("SELECTOR"));
        }
        (
            "goto-definition",
            BenchmarkResponse::GotoDefinition(Some(GotoDefinitionResponse::Array(locations))),
        ) => {
            assert_eq!(locations.len(), 1);
            assert!(locations[0].uri.path().ends_with("/src/libraries/UnifapV2Library.sol"));
        }
        ("references", BenchmarkResponse::References(Some(locations))) => {
            assert_eq!(locations.len(), 4);
            assert_eq!(
                locations
                    .iter()
                    .filter(|location| location.uri.path().ends_with("/src/UnifapV2Factory.sol"))
                    .count(),
                1
            );
            assert_eq!(
                locations
                    .iter()
                    .filter(|location| {
                        location.uri.path().ends_with("/src/test/UnifapV2Factory.t.sol")
                    })
                    .count(),
                3
            );
        }
        ("workspace-symbols", BenchmarkResponse::WorkspaceSymbols(symbols)) => {
            assert_eq!(symbols.len(), 3);
            assert!(symbols.iter().all(|symbol| symbol.name.contains("UnifapV2Pair")));
            assert!(symbols.iter().any(|symbol| symbol.name == "UnifapV2Pair"));
        }
        _ => panic!("unexpected `{name}` response for the unifap-v2 benchmark"),
    }
}

fn assert_clean(analysis: &solar_lsp::BenchmarkAnalysis) {
    assert_eq!(analysis.diagnostic_count(), 0, "{}", analysis.diagnostic_fingerprint());
}

fn optimism_requests(c: &mut Criterion) {
    // The flattened Optimism corpus contains conflicting declarations from different dependency
    // versions. Its original Predeploys module is self-contained and can be analyzed unchanged.
    let source = OPTIMISM_SOURCE
        .split_once("// src/libraries/Predeploys.sol\n")
        .unwrap()
        .1
        .split_once("// src/cannon/PreimageKeyLib.sol\n")
        .unwrap()
        .0;
    let project = BenchmarkProject::from_source(source.to_owned());
    let analysis = project.clone().analyze();
    assert_clean(&analysis);
    let (uri, position) = project
        .unique_anchor("benchmark.sol", "_addr) internal pure returns (string memory out_)")
        .unwrap();
    let mut requests = BenchmarkRenameRequests::new(project.clone(), uri.clone(), position);
    let response = requests.run().expect("the Predeploys getName argument should be renameable");
    let edits = &response.changes.as_ref().unwrap()[&uri];
    assert_eq!(edits.len(), 31);
    assert!(edits.iter().all(|edit| edit.new_text == "renamed"));
    c.benchmark_group("lsp/rename").bench_function(
        BenchmarkId::from_parameter("optimism-predeploys"),
        |b| {
            b.iter(|| black_box(requests.run()));
        },
    );
    c.benchmark_group("lsp/project-analysis").bench_function(
        BenchmarkId::from_parameter("optimism-predeploys"),
        |b| {
            b.iter_batched(
                || project.clone(),
                |project| black_box(project.analyze()),
                BatchSize::PerIteration,
            );
        },
    );
}

fn unifap_benches(c: &mut Criterion) {
    let project = unifap_project();
    let edit = project
        .replacement_edit(UNIFAP_PAIR, "MINIMUM_LIQUIDITY = 1e3", "MINIMUM_LIQUIDITY = 1e4")
        .expect("the edit anchor should be unique");
    let document_change =
        project.document_change(&edit).expect("the benchmark document change should be prepared");
    let requests = unifap_requests(&project);

    let analysis = project.clone().analyze();
    assert_clean(&analysis);
    for (name, request) in &requests {
        assert_unifap_response(name, analysis.execute(request));
    }

    {
        let mut edited_project = project.clone();
        edited_project.apply_edit(&edit).expect("the benchmark edit should apply");
        edited_project
            .unique_anchor(UNIFAP_PAIR, "MINIMUM_LIQUIDITY = 1e4")
            .expect("the edited source should contain the replacement");
        let edited_analysis = edited_project.analyze();
        assert_clean(&edited_analysis);
    }

    let mut group = c.benchmark_group("lsp/project-analysis");
    group.bench_function(BenchmarkId::from_parameter(UNIFAP_PROJECT), |b| {
        b.iter_batched(
            || project.clone(),
            |project| black_box(project.analyze()),
            BatchSize::PerIteration,
        );
    });
    group.finish();

    let mut group = c.benchmark_group("lsp/project-analysis-after-edit");
    group.bench_function(BenchmarkId::from_parameter(UNIFAP_PROJECT), |b| {
        b.iter_batched(
            || {
                let mut project = project.clone();
                project.apply_edit(&edit).expect("the benchmark edit should apply");
                project
            },
            |project| black_box(project.analyze()),
            BatchSize::PerIteration,
        );
    });
    group.finish();

    let mut group = c.benchmark_group("lsp/project-edit-application");
    group.throughput(Throughput::Elements(1));
    group.bench_function(BenchmarkId::from_parameter(UNIFAP_PROJECT), |b| {
        b.iter_batched(
            || document_change.clone(),
            |change| black_box(change.apply()),
            BatchSize::PerIteration,
        );
    });
    group.finish();

    let mut group = c.benchmark_group("lsp/symbol-table-queries");
    for (name, request) in &requests {
        let id = format!("{UNIFAP_PROJECT}-{name}");
        group.bench_with_input(BenchmarkId::from_parameter(id), request, |b, request| {
            b.iter(|| black_box(analysis.execute(black_box(request))))
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    analysis_build,
    rename_candidate_queries,
    rename_requests,
    completion_queries,
    member_completion_queries,
    signature_help_requests,
    signature_help_moving_cursors,
    code_lens_queries,
    document_symbol_queries,
    type_hierarchy_queries,
    call_hierarchy_queries,
    call_hierarchy_requests,
    import_path_queries,
    bounded_workspace_discovery,
    symbol_table_aggregation,
    burst_hover,
    folding_range,
    selection_range,
    open_document_selection_range,
    workspace_diagnostic_hot_paths,
    incoming_document_changes,
    open_document_analysis_batches,
    repeated_analysis,
    workspace_index_reuse,
    single_workspace_index_reuse,
    workspace_path_queries,
    optimism_requests,
    unifap_benches
);
criterion_main!(benches);
