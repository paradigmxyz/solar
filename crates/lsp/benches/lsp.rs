//! Name every case `lsp/<operation>[<scenario>]` using parameter IDs so CodSpeed retains the
//! operation. Include the corpus or workload size with units, plus any cache or cursor state.

#![allow(unused_crate_dependencies)]

use criterion::{
    BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
    measurement::WallTime,
};
use crop::Rope;
use lsp_types::{
    GotoDefinitionResponse, HoverContents, OneOf, Position, Range, SelectionRange,
    TextDocumentContentChangeEvent, Url,
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
use std::{
    fmt::{Display, Write as _},
    fs,
    hint::black_box,
    path::{Path, PathBuf},
};

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
const BENCHMARK_FILE: &str = "benchmark.sol";
const UNIFAP_PROJECT: &str = "unifap-v2";
const UNIFAP_ROUTER: &str = "src/UnifapV2Router.sol";
const UNIFAP_PAIR: &str = "src/UnifapV2Pair.sol";
const UNIFAP_FACTORY: &str = "src/UnifapV2Factory.sol";
const TRANSFER_A: &str = "_safeTransferFrom(tokenA, msg.sender, pair, amountA)";
const TRANSFER_B: &str = "_safeTransferFrom(tokenB, msg.sender, pair, amountB)";
const DEPENDENCY_MAIN: &str = "import \"./lib/Dependency.sol\";\ncontract Main is Dependency {\nfunction target() internal {}\n";
const OPTIMISM_SOURCE: &str = include_str!("../../../testdata/Optimism.sol");
const UNISWAP_SOURCE: &str = include_str!("../../../testdata/UniswapV3.sol");

type Group<'a> = BenchmarkGroup<'a, WallTime>;

/// Time `routine` against state prepared once, outside the measurement.
fn bench<O>(group: &mut Group<'_>, name: impl Display, mut routine: impl FnMut() -> O) {
    group.bench_function(BenchmarkId::from_parameter(name), |b| b.iter(&mut routine));
}

/// Time `routine` on a fresh `setup` value per iteration, excluding the setup.
fn bench_batched<I, O>(
    group: &mut Group<'_>,
    name: impl Display,
    mut setup: impl FnMut() -> I,
    mut routine: impl FnMut(I) -> O,
) {
    group.bench_function(BenchmarkId::from_parameter(name), |b| {
        b.iter_batched(&mut setup, &mut routine, BatchSize::PerIteration)
    });
}

/// Like [`bench_batched`], but the routine borrows the prepared value.
fn bench_batched_ref<I, O>(
    group: &mut Group<'_>,
    name: impl Display,
    mut setup: impl FnMut() -> I,
    mut routine: impl FnMut(&mut I) -> O,
) {
    group.bench_function(BenchmarkId::from_parameter(name), |b| {
        b.iter_batched_ref(&mut setup, &mut routine, BatchSize::PerIteration)
    });
}

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
        push_line(&format!(
            "    function {name}(uint256 first, uint256 second, address account) public pure returns (uint256 total, address owner) {{"
        ));
        hover_anchors.push(format!("{name}(uint256 first"));
        push_line("        total = first + second;");
        push_line("        owner = account;");
        push_line("    }");
    }

    push_line("    function exercise() public pure {");
    for index in 0..function_count {
        let name = format!("function_{index:04}");
        push_line(&format!("        {name}(1, 2, address(0));"));
        hover_anchors.push(format!("{name}(1, 2, address(0))"));
    }
    push_line("    }");
    push_line("}");
    let project = BenchmarkProject::from_source(source.clone());
    let hover_positions = hover_anchors
        .into_iter()
        .map(|anchor| {
            let (_, position) = project
                .unique_anchor(BENCHMARK_FILE, &anchor)
                .expect("generated hover anchors should be unique");
            (position.line, position.character)
        })
        .collect();
    BenchmarkSource { source, project, hover_positions }
}

/// `prefix`, then `caller_count` one-line functions calling `target()`, then `suffix`.
fn callers_source(prefix: &str, caller_count: usize, mutability: &str, suffix: &str) -> String {
    let mut source = prefix.to_owned();
    for index in 0..caller_count {
        writeln!(source, "function caller{index}() public {mutability}{{ target(); }}").unwrap();
    }
    source + suffix
}

/// A `Root` contract whose `target()` is called by `caller_count` functions.
fn root_callers_source(caller_count: usize, mutability: &str) -> String {
    let prefix = format!("contract Root {{ function target() internal {mutability}{{}}\n");
    callers_source(&prefix, caller_count, mutability, "}\n")
}

/// Resolve a unique `needle`, then move right past the ASCII `prefix` of that match.
fn anchor(project: &BenchmarkProject, path: &str, needle: &str, prefix: &str) -> (Url, Position) {
    let (uri, mut position) = project.unique_anchor(path, needle).unwrap();
    position.character += prefix.len() as u32;
    (uri, position)
}

fn position_at(source: &str, offset: usize) -> Position {
    let prefix = &source[..offset];
    Position::new(
        prefix.bytes().filter(|&byte| byte == b'\n').count() as u32,
        prefix.rsplit('\n').next().unwrap().encode_utf16().count() as u32,
    )
}

fn assert_clean(analysis: &BenchmarkAnalysis) {
    assert_eq!(analysis.diagnostic_count(), 0, "{}", analysis.diagnostic_fingerprint());
}

fn analyze(project: BenchmarkProject) -> BenchmarkAnalysis {
    let analysis = project.analyze();
    assert_clean(&analysis);
    analysis
}

fn analysis_build(c: &mut Criterion) {
    let call = "function_0000(1, 2, address(0));";
    let mut workloads = ANALYSIS_FUNCTION_COUNTS
        .map(|count| (format!("{count}-functions"), benchmark_source(count).source))
        .to_vec();
    workloads.push((
        "1-function-256-repeated-calls".into(),
        benchmark_source(1).source.replace(call, &call.repeat(256)),
    ));
    let mut group = c.benchmark_group("lsp/analysis-build");
    for (name, source) in workloads {
        assert_clean(&BenchmarkAnalysis::from_source(source.clone()));
        group.throughput(Throughput::Bytes(source.len() as u64));
        bench_batched(
            &mut group,
            name,
            || source.clone(),
            |source| BenchmarkAnalysis::from_source(black_box(source)),
        );
    }
    group.finish();
}

fn call_hierarchy_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/call-hierarchy");
    for caller_count in [128, 2_048] {
        let project = BenchmarkProject::from_source(root_callers_source(caller_count, ""));
        let (uri, target) = project.unique_anchor(BENCHMARK_FILE, "target() internal").unwrap();
        let analysis = analyze(project.clone());
        assert_eq!(analysis.incoming_calls(&uri, target).len(), caller_count);
        bench(&mut group, format!("{caller_count}-callers-incoming"), || {
            analysis.incoming_calls(black_box(&uri), black_box(target))
        });

        let call_prefix = format!("function caller{}() public {{ ", caller_count - 1);
        let caller_line = format!("{call_prefix}target(); }}");
        let (_, body) = anchor(&project, BENCHMARK_FILE, &caller_line, "function ");
        let (_, call) = anchor(&project, BENCHMARK_FILE, &caller_line, &call_prefix);
        for (location, position) in [("body", body), ("callsite", call)] {
            assert_eq!(analysis.prepare_call_hierarchy(&uri, position).unwrap().len(), 1);
            bench(&mut group, format!("{caller_count}-callers-prepare-{location}"), || {
                analysis.prepare_call_hierarchy(black_box(&uri), black_box(position))
            });
        }
        bench_batched(
            &mut group,
            format!("{caller_count}-callers-prepare-body-first-request"),
            || analysis.clone(),
            |cold| cold.prepare_call_hierarchy(&uri, call),
        );
    }
    group.finish();
}

fn call_hierarchy_requests(c: &mut Criterion) {
    let project = unifap_project();
    let mut requests = BenchmarkCallHierarchyRequests::new(analyze(project.clone()));
    let (uri, declaration) =
        anchor(&project, UNIFAP_ROUTER, "function _safeTransferFrom(", "function ");
    let (_, body) = project.unique_anchor(UNIFAP_ROUTER, "success = IERC20(token)").unwrap();
    let (_, callsite) = project.unique_anchor(UNIFAP_ROUTER, TRANSFER_A).unwrap();
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
        let (_, start) = anchor(&project, UNIFAP_ROUTER, call, &call[..call.find(name).unwrap()]);
        Range::new(start, Position::new(start.line, start.character + name.len() as u32))
    };
    let incoming = requests.incoming(helper).unwrap();
    assert_eq!(incoming.len(), 2);
    for (call, name, expected_calls) in [
        (&incoming[0], "addLiquidity", &[TRANSFER_A, TRANSFER_B][..]),
        (
            &incoming[1],
            "removeLiquidity",
            &["_safeTransferFrom(address(pair), msg.sender, address(pair), liquidity)"][..],
        ),
    ] {
        let (_, position) =
            anchor(&project, UNIFAP_ROUTER, &format!("function {name}("), "function ");
        assert_eq!(requests.prepare(&uri, position), Some(vec![call.from.clone()]));
        assert_eq!(call.from.name, name);
        assert_eq!(
            call.from_ranges,
            expected_calls.iter().map(|call| call_range(call, &helper.name)).collect::<Vec<_>>()
        );
    }
    let outgoing = requests.outgoing(helper).unwrap();
    assert_eq!(outgoing.len(), 1);
    let (token_uri, token_position) =
        anchor(&project, "src/interfaces/IERC20.sol", "function transferFrom(", "function ");
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
        bench(&mut group, format!("unifap-v2-prepare-{location}"), || {
            requests.prepare(black_box(&uri), black_box(position))
        });
    }
    bench(&mut group, "unifap-v2-incoming", || requests.incoming(black_box(helper)));
    bench(&mut group, "unifap-v2-outgoing", || requests.outgoing(black_box(helper)));
    group.finish();

    let mut group = c.benchmark_group("lsp/call-hierarchy-expand");
    group.throughput(Throughput::Elements(5));
    bench(&mut group, "unifap-v2-transfer-helper", || {
        let items = requests.prepare(black_box(&uri), black_box(callsite)).unwrap();
        let callers = requests.incoming(black_box(&items[0])).unwrap();
        for caller in &callers {
            black_box(requests.outgoing(black_box(&caller.from)));
        }
        black_box(requests.outgoing(black_box(&items[0])));
    });
    group.finish();

    let mut group = c.benchmark_group("lsp/call-hierarchy-first-request");
    bench_batched_ref(
        &mut group,
        "unifap-v2-router",
        || requests.before_first_request(),
        |requests| requests.prepare(black_box(&uri), black_box(callsite)),
    );
    group.finish();
}

fn rename_candidate_queries(c: &mut Criterion) {
    let project = BenchmarkProject::from_source(root_callers_source(2_048, ""));
    let call_prefix = "function caller2047() public { ";
    let (uri, hit) =
        anchor(&project, BENCHMARK_FILE, &format!("{call_prefix}target(); }}"), call_prefix);
    let analysis = analyze(project);
    let Some((range, edit_count)) = analysis.rename_candidate(&uri, hit) else {
        panic!("rename candidate should resolve at the final call site");
    };
    assert_eq!(edit_count, 2_049);
    assert!(range.start <= hit && hit < range.end);
    let miss = Position::new(hit.line + 1, 0);
    assert!(analysis.rename_candidate(&uri, miss).is_none());

    let mut group = c.benchmark_group("lsp/rename-candidate");
    for (name, position) in [("hit", hit), ("miss", miss)] {
        bench(&mut group, format!("2048-callers-{name}-near-eof"), || {
            analysis.rename_candidate(black_box(&uri), black_box(position))
        });
    }
    group.finish();
}

fn rename_requests(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/rename");
    for reference_count in [0, 64, 2_048] {
        let project = BenchmarkProject::from_source(root_callers_source(reference_count, "pure "));
        let (uri, position) = project.unique_anchor(BENCHMARK_FILE, "target() internal").unwrap();
        let mut requests = BenchmarkRenameRequests::new(project, uri.clone(), position);
        let response = requests.run().expect("the target should be renameable");
        let edits = &response.changes.as_ref().unwrap()[&uri];
        assert_eq!(edits.len(), reference_count + 1);
        assert!(edits.iter().all(|edit| edit.new_text == "renamed"));
        bench(&mut group, format!("{reference_count}-references"), || requests.run());
    }

    let project = unifap_project();
    let (uri, position) = project.unique_anchor(UNIFAP_ROUTER, "_safeTransferFrom(\n").unwrap();
    let mut requests = BenchmarkRenameRequests::new(project, uri, position);
    let edits = requests.run().expect("the router helper should be renameable").changes.unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits.values().next().unwrap().len(), 4);
    bench(&mut group, "unifap-v2-router", || requests.run());
    group.finish();
}

fn type_hierarchy_queries(c: &mut Criterion) {
    let mut source = String::from("contract Root {}\n");
    for index in 0..128 {
        writeln!(source, "contract Child{index} is Root {{}}").unwrap();
    }
    let project = BenchmarkProject::from_source(source);
    let (uri, position) =
        project.unique_anchor(BENCHMARK_FILE, "Root {}\ncontract Child0").unwrap();
    let analysis = analyze(project);
    assert_eq!(analysis.type_hierarchy(&uri, position).len(), 128);
    let mut group = c.benchmark_group("lsp/type-hierarchy");
    bench(&mut group, "128-subtypes", || {
        analysis.type_hierarchy(black_box(&uri), black_box(position))
    });
    group.finish();
}

fn code_lens_queries(c: &mut Criterion) {
    // Keep an unqueried clone of each analysis for the first-request samples.
    let mut workloads = Vec::new();
    let mut prepare = |name: String, project: BenchmarkProject, path, needle| {
        let (uri, position) = project.unique_anchor(path, needle).unwrap();
        let analysis = analyze(project);
        let cold = analysis.clone();
        let lenses = analysis.code_lenses(&uri);
        workloads.push((name, cold, analysis, uri));
        (position, lenses)
    };
    let (_, lenses) = prepare(
        "256-functions".into(),
        benchmark_source(HOVER_FUNCTION_COUNT).project,
        BENCHMARK_FILE,
        "function_0255(1, 2, address(0))",
    );
    assert!(lenses.len() >= HOVER_FUNCTION_COUNT);
    for reference_count in [64, 1_024, 16_384] {
        let source = format!(
            "contract RepeatedReferences {{\nfunction target() internal pure {{}}\nfunction exercise() public pure {{\n{}}}\n}}\n",
            "target();\n".repeat(reference_count)
        );
        let (position, lenses) = prepare(
            format!("{reference_count}-references"),
            BenchmarkProject::from_source(source),
            BENCHMARK_FILE,
            "target() internal",
        );
        assert_eq!(lenses.len(), 4);
        assert!(lenses.iter().any(|lens| {
            lens.range.start == position
                && lens.command.as_ref().unwrap().title == format!("{reference_count} references")
        }));
    }
    let (_, lenses) = prepare("unifap-v2-pair".into(), unifap_project(), UNIFAP_PAIR, "SELECTOR");
    assert!(!lenses.is_empty());

    let mut group = c.benchmark_group("lsp/code-lens");
    for (name, _, analysis, uri) in &workloads {
        bench(&mut group, name, || analysis.code_lenses(black_box(uri)));
    }
    group.finish();
    let mut group = c.benchmark_group("lsp/code-lens-first-request");
    for (name, cold, _, uri) in &workloads {
        bench_batched_ref(
            &mut group,
            name,
            || cold.clone(),
            |analysis| analysis.code_lenses(black_box(uri)),
        );
    }
    group.finish();
}

fn document_symbol_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/document-symbol");
    for function_count in [256, 1_024] {
        let project = benchmark_source(function_count).project;
        let (uri, _) = project.unique_anchor(BENCHMARK_FILE, "contract Benchmark").unwrap();
        let analysis = analyze(project);
        let symbols = analysis.document_symbols(&uri);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].children.as_ref().map_or(0, Vec::len), function_count + 1);
        group.throughput(Throughput::Elements(function_count as u64));
        bench(&mut group, format!("{function_count}-functions"), || {
            analysis.document_symbols(black_box(&uri))
        });
    }
    group.finish();
}

fn import_path_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/import-path");
    let cursor = OPTIMISM_SOURCE.rfind('}').unwrap();
    assert!(!benchmark_import_path_at(OPTIMISM_SOURCE, cursor));
    bench(&mut group, "optimism-non-import-at-end", || {
        benchmark_import_path_at(black_box(OPTIMISM_SOURCE), black_box(cursor))
    });
    group.finish();
}

fn completion_queries(c: &mut Criterion) {
    let project = benchmark_source(HOVER_FUNCTION_COUNT).project;
    let (uri, position) =
        project.unique_anchor(BENCHMARK_FILE, "function_0255(1, 2, address(0))").unwrap();
    let analysis = analyze(project);
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
        bench(&mut group, format!("{HOVER_FUNCTION_COUNT}-functions-{name}"), || {
            analysis.completions(black_box(&uri), black_box(position), black_box(prefix))
        });
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
        let needle = format!("value.field; // {}", access_count - 1);
        let (uri, position) = anchor(&project, BENCHMARK_FILE, &needle, "value.field");
        let analysis = analyze(project);
        let items = analysis.completions(&uri, position, "field");
        assert_eq!(items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(), ["field"]);
        bench(&mut group, format!("{access_count}-member-accesses"), || {
            analysis.completions(black_box(&uri), black_box(position), black_box("field"))
        });
    }
    group.finish();
}

fn signature_help_requests(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/signature-help");
    let mut add = |name: &str,
                   project: BenchmarkProject,
                   path: &str,
                   needle: &str,
                   prefix: &str,
                   label: &str| {
        let (uri, position) = anchor(&project, path, needle, prefix);
        let mut requests = BenchmarkSignatureHelpRequests::new(project, uri, position);
        let response = requests.run().expect("benchmark call should have signature help");
        assert_eq!(response.active_parameter, Some(1));
        assert_eq!(response.signatures.len(), 1);
        assert!(response.signatures[0].label.starts_with(label));
        bench(&mut group, name, || requests.run());
        (requests, response)
    };
    for function_count in [64, 256, 1024] {
        let callee = format!("function_{:04}(", function_count - 1);
        add(
            &format!("{function_count}-functions"),
            benchmark_source(function_count).project,
            BENCHMARK_FILE,
            &format!("{callee}1, 2, address(0))"),
            &format!("{callee}1, "),
            &format!("function {callee}"),
        );
    }
    let source = format!(
        "contract Repeated {{ function target(uint256 first, uint256 second) public {{}} function exercise() public {{\n{}target(1, 2); // final\n}}\n}}\n",
        "target(1, 2);\n".repeat(1_023)
    );
    add(
        "1024-repeated-calls",
        BenchmarkProject::from_source(source),
        BENCHMARK_FILE,
        "target(1, 2); // final",
        "target(1, ",
        "function target(",
    );
    let (requests, response) = add(
        "unifap-v2-router",
        unifap_project(),
        UNIFAP_ROUTER,
        TRANSFER_B,
        "_safeTransferFrom(tokenB, ",
        "function _safeTransferFrom(",
    );
    assert_eq!(requests.after_edit().run(), Some(response));
    bench_batched_ref(
        &mut group,
        "unifap-v2-router-after-edit",
        || requests.after_edit(),
        |requests| requests.run(),
    );
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
        let (uri, early) =
            anchor(&project, BENCHMARK_FILE, "function_0000(3, 4, address(0))", "function_0000(");
        let mut positions = Vec::new();
        for index in function_count - 8..function_count {
            let callee = format!("function_{index:04}(");
            let needle = format!("{callee}1, 2, address(0))");
            for (parameter, prefix) in ["", "1, ", "1, 2, "].into_iter().enumerate() {
                let (_, position) =
                    anchor(&project, BENCHMARK_FILE, &needle, &format!("{callee}{prefix}"));
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
    let transfer = "function _safeTransferFrom(address token, address from, address to, uint256 amount) internal returns (bool success)";
    for (call, arguments, label) in [
        (TRANSFER_A, &["tokenA", "msg.sender", "pair", "amountA"][..], transfer),
        (TRANSFER_B, &["tokenB", "msg.sender", "pair", "amountB"][..], transfer),
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
        for (parameter, argument) in arguments.iter().enumerate() {
            let prefix = &call[..call.find(argument).unwrap()];
            let (_, position) = anchor(&project, UNIFAP_ROUTER, call, prefix);
            positions.push((position, parameter as u32, label.to_owned()));
        }
    }
    let early = positions.first().unwrap().clone();
    let late = positions.last().unwrap().clone();
    let (uri, _) = project.unique_anchor(UNIFAP_ROUTER, TRANSFER_A).unwrap();
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
        bench(&mut group, format!("{name}-{}-cursor-positions", positions.len()), || {
            for (position, _, _) in black_box(&*positions) {
                black_box(requests.run_at(black_box(*position)));
            }
        });
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
                bench_batched_ref(
                    &mut group,
                    format!("{name}-{location}-cursor"),
                    || if edited { requests.after_edit() } else { requests.before_first_request() },
                    |requests| requests.run_at(black_box(position)),
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
    bench(&mut group, "foundry-10000-import-only-files", || {
        BenchmarkWorkspaceDiscovery::run(black_box(temp.path()))
    });
    group.finish();
}

fn symbol_table_aggregation(c: &mut Criterion) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/aggregation");
    let source = benchmark_source(AGGREGATION_FUNCTION_COUNT).source;
    let batch_path = |index| root.join(format!("batch-{index}.sol"));
    let project = BenchmarkProject::from_sources(
        CompileOpts { base_path: Some(root.clone()), ..Default::default() },
        (0..AGGREGATION_BATCH_COUNT).map(|index| (batch_path(index), source.clone())),
    )
    .expect("the aggregation benchmark project should be valid");
    let batches = project.clone().analyze_file_batches();
    assert_eq!(batches.len(), AGGREGATION_BATCH_COUNT);
    assert!(batches.iter().all(|batch| batch.diagnostic_count() == 0));

    let query = format!("function_{:04}", AGGREGATION_FUNCTION_COUNT - 1);
    let per_batch = format!("{AGGREGATION_FUNCTION_COUNT}-functions-per-batch");
    let mut group = c.benchmark_group("lsp/symbol-table-aggregation");
    for batch_count in [1, AGGREGATION_BATCH_COUNT] {
        let merged = BenchmarkAnalysis::merge(batches[..batch_count].to_vec());
        assert_clean(&merged);
        let BenchmarkResponse::WorkspaceSymbols(symbols) =
            merged.execute(&BenchmarkRequest::WorkspaceSymbols { query: query.clone() })
        else {
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
            .map(|index| Url::from_file_path(batch_path(index)).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(uris, expected_uris);

        group.throughput(Throughput::Elements(batch_count as u64));
        bench_batched(
            &mut group,
            format!("{batch_count}-batches-{per_batch}"),
            || batches[..batch_count].to_vec(),
            |batches| BenchmarkAnalysis::merge(black_box(batches)),
        );
    }
    group.finish();

    let mut group = c.benchmark_group("lsp/project-analysis-batched");
    bench_batched(
        &mut group,
        format!("{AGGREGATION_BATCH_COUNT}-batches-{per_batch}"),
        || project.clone(),
        |project| BenchmarkAnalysis::merge(black_box(project.analyze_file_batches())),
    );
    group.finish();
}

fn burst_hover(c: &mut Criterion) {
    let fixture = benchmark_source(HOVER_FUNCTION_COUNT);
    let analysis = analyze(fixture.project);
    let positions = fixture.hover_positions;
    assert_eq!(positions.len(), HOVER_FUNCTION_COUNT * 2);
    assert!(positions.iter().all(|&(line, character)| analysis.hover(line, character).is_some()));

    let mut group = c.benchmark_group("lsp/burst-hover");
    group.throughput(Throughput::Elements(positions.len() as u64));
    bench(
        &mut group,
        format!("{HOVER_FUNCTION_COUNT}-functions-{}-requests", positions.len()),
        || {
            let analysis = black_box(&analysis);
            for &(line, character) in black_box(&positions) {
                black_box(analysis.hover(black_box(line), black_box(character)));
            }
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
    bench_batched(
        &mut group,
        "optimism-start-1-position",
        || OPTIMISM_SOURCE.to_owned(),
        |source| benchmark_selection_ranges(black_box(source), black_box(&positions)),
    );
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
    assert!(!clean_ranges.is_empty());
    assert_eq!(
        benchmark_folding_ranges(incomplete.clone()).len(),
        INCOMPLETE_FOLDING_CONTRACT_COUNT * 2 * 3
    );
    assert!(benchmark_folding_ranges(minified.clone()).is_empty());
    assert_eq!(benchmark_folding_ranges_from_rope(open_rope.clone()), clean_ranges);

    let mut group = c.benchmark_group("lsp/folding-range");
    for (name, source) in [
        ("optimism-clean".to_owned(), clean),
        (format!("{}-contracts-incomplete", INCOMPLETE_FOLDING_CONTRACT_COUNT * 2), incomplete),
        (format!("{MINIFIED_FOLDING_FUNCTION_COUNT}-functions-minified"), minified),
    ] {
        group.throughput(Throughput::Bytes(source.len() as u64));
        bench_batched(
            &mut group,
            name,
            || source.clone(),
            |source| benchmark_folding_ranges(black_box(source)),
        );
    }
    group.throughput(Throughput::Bytes(OPTIMISM_SOURCE.len() as u64));
    bench_batched(
        &mut group,
        "optimism-open-document-rope-to-string",
        || open_rope.clone(),
        |rope| benchmark_folding_ranges(black_box(rope_to_string(&rope))),
    );
    bench_batched(
        &mut group,
        "optimism-open-document-rope-snapshot",
        || open_rope.clone(),
        |rope| benchmark_folding_ranges_from_rope(black_box(rope)),
    );
    group.finish();

    let requests = BenchmarkFoldingRangeRequests::new(OPTIMISM_SOURCE.to_owned());
    assert_eq!(requests.run(), clean_ranges);
    let mut group = c.benchmark_group("lsp/open-document-folding-range");
    group.throughput(Throughput::Bytes(OPTIMISM_SOURCE.len() as u64));
    bench(&mut group, "optimism-unchanged", || requests.run());
    bench_batched_ref(
        &mut group,
        "optimism-first-request",
        || BenchmarkFoldingRangeRequests::new(OPTIMISM_SOURCE.to_owned()),
        |requests| requests.run(),
    );
    group.finish();
}

/// Prepare open-document selection-range requests that match the stateless kernel.
fn selection_requests(
    source: &str,
    positions: &[Position],
) -> (BenchmarkSelectionRangeRequests, Vec<SelectionRange>) {
    let requests = BenchmarkSelectionRangeRequests::new(source.to_owned(), positions.to_vec());
    let expected = benchmark_selection_ranges(source.to_owned(), positions)
        .expect("the benchmark positions should be valid");
    assert_eq!(requests.run().as_ref(), Some(&expected));
    (requests, expected)
}

fn open_document_selection_range(c: &mut Criterion) {
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
        let (requests, ranges) = selection_requests(OPTIMISM_SOURCE, &positions);
        assert_eq!(ranges.len(), positions.len());
        for (range, position) in ranges.iter().zip(&positions) {
            assert!(range.range.start <= *position && *position < range.range.end);
            if *position != start {
                assert!(range.parent.is_some());
            }
        }
        bench(&mut group, name, || black_box(&requests).run());
    }
    for (name, source) in [
        ("uniswap-v3", UNISWAP_SOURCE),
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
        let (requests, expected) = selection_requests(source, &[position_at(source, offset)]);
        assert!(expected[0].parent.is_some());
        group.throughput(Throughput::Bytes(source.len() as u64));
        bench(&mut group, name, || black_box(&requests).run());
    }
    group.finish();

    let mut group = c.benchmark_group("lsp/open-document-selection-range-line-layout");
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
        let (requests, expected) =
            selection_requests(&source, &[position_at(&source, literal_start + 2)]);
        assert_eq!(
            expected[0].range,
            Range::new(
                position_at(&source, literal_start),
                position_at(&source, literal_start + "123456".len()),
            )
        );
        assert!(expected[0].parent.is_some());
        group.throughput(Throughput::Bytes(source.len() as u64));
        bench(&mut group, format!("1024-functions-{name}-1-position"), || {
            black_box(&requests).run()
        });
    }
    group.finish();

    let mut group = c.benchmark_group("lsp/open-document-selection-range-cold");
    group.throughput(Throughput::Bytes(OPTIMISM_SOURCE.len() as u64));
    bench_batched_ref(
        &mut group,
        "optimism-middle-1-position",
        || BenchmarkSelectionRangeRequests::new(OPTIMISM_SOURCE.to_owned(), [middle]),
        |requests| requests.run(),
    );
    group.finish();
}

fn workspace_diagnostic_hot_paths(c: &mut Criterion) {
    let source = OPTIMISM_SOURCE.to_owned();
    assert_eq!(BenchmarkDocumentUpdate::from_source(source.clone()).apply(), 1);
    let mut group = c.benchmark_group("lsp/unchanged-document-update");
    group.throughput(Throughput::Bytes(source.len() as u64));
    bench_batched(
        &mut group,
        "optimism",
        || BenchmarkDocumentUpdate::from_source(source.clone()),
        |update| update.apply(),
    );
    group.finish();

    let mut group = c.benchmark_group("lsp/workspace-diagnostic-reports");
    for document_count in [256, 4096] {
        assert_eq!(
            BenchmarkWorkspaceReports::new(document_count).generate(),
            document_count + document_count / 4
        );
        group.throughput(Throughput::Elements(document_count as u64));
        bench_batched(
            &mut group,
            format!("{document_count}-documents"),
            || BenchmarkWorkspaceReports::new(document_count),
            |reports| reports.generate(),
        );
    }
    group.finish();
}

fn incoming_document_changes(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/incoming-document-changes");
    let mut add = |name: String, edit_count, change: BenchmarkDocumentChange, expected: &str| {
        assert_eq!(change.clone().apply().contents().to_string(), expected);
        group.throughput(Throughput::Elements(edit_count as u64));
        bench_batched(&mut group, name, || change.clone(), |change| change.apply());
    };
    for (name, source, identifier, occurrence_count, edit_counts) in [
        ("optimism-predeploys", OPTIMISM_SOURCE, "Predeploys", 377, &[1, 8, 64, 377][..]),
        ("uniswap-tickmath", UNISWAP_SOURCE, "TickMath", 20, &[1, 20][..]),
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
        let contents = Rope::from(source);
        let replacement = format!("{identifier}Renamed");
        for &edit_count in edit_counts {
            let mut expected = source.to_owned();
            let mut changes = Vec::with_capacity(edit_count);
            // Clients apply independent replacements from the end to preserve earlier positions.
            for range in occurrences.iter().rev().take(edit_count) {
                changes.push(TextDocumentContentChangeEvent {
                    range: Some(Range::new(
                        position_at(source, range.start),
                        position_at(source, range.end),
                    )),
                    range_length: None,
                    text: replacement.clone(),
                });
                expected.replace_range(range.clone(), &replacement);
            }
            let change = BenchmarkDocumentChange::from_changes(contents.clone(), changes);
            add(format!("{name}-{edit_count}-edits"), edit_count, change, &expected);
        }
    }

    // Overlapping ranges must retain the sequential LSP behavior and exercise the fallback path.
    let changes = [((0, 2), (0, 4), "X"), ((0, 1), (0, 3), "Y")]
        .map(|(start, end, text)| TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(start.0, start.1), Position::new(end.0, end.1))),
            range_length: None,
            text: text.into(),
        })
        .to_vec();
    let change = BenchmarkDocumentChange::from_changes(Rope::from("abcdef\nghijkl\n"), changes);
    add("2-overlapping-edits-sequential-fallback".into(), 2, change, "aYef\nghijkl\n");
    group.finish();
}

fn open_document_analysis_batches(c: &mut Criterion) {
    let documents = BenchmarkOpenDocuments::new(OPEN_DOCUMENT_COUNT, OPEN_DOCUMENT_BYTES);
    // Prime the initial snapshot so timing measures reuse across later analysis epochs.
    assert!(documents.build_analysis_batches() >= documents.source_bytes());

    let mut group = c.benchmark_group("lsp/open-document-analysis-batches");
    group.throughput(Throughput::Bytes(documents.source_bytes() as u64));
    bench(
        &mut group,
        format!("{OPEN_DOCUMENT_COUNT}-documents-{OPEN_DOCUMENT_BYTES}-bytes-per-document"),
        || documents.build_analysis_batches(),
    );
    group.finish();
}

fn repeated_analysis(c: &mut Criterion) {
    let source = benchmark_source(256).source;
    let mut group = c.benchmark_group("lsp/incremental-analysis");
    bench_batched(
        &mut group,
        "256-functions-cold",
        || BenchmarkRepeatedAnalysis::new(source.clone()),
        |mut analysis| analysis.run(),
    );

    let mut analysis = BenchmarkRepeatedAnalysis::new(source);
    assert!(analysis.run());
    bench(&mut group, "256-functions-unchanged", || analysis.run());
    bench(&mut group, "256-functions-reverted-edit", || {
        analysis.edit_and_revert();
        analysis.run()
    });
    group.finish();
}

fn copy_sources(source: &Path, destination: &Path) {
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

fn single_workspace_index_reuse(c: &mut Criterion) {
    let temp = tempfile::tempdir().expect("single workspace benchmark directory");
    let root = temp.path().to_path_buf();
    fs::create_dir(root.join("lib")).unwrap();
    let generated = callers_source(DEPENDENCY_MAIN, 256, "", "}\n");
    fs::write(root.join("lib/Dependency.sol"), "contract Dependency {}\n").unwrap();
    copy_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/foundry/unifap-v2/src"),
        &root.join("lib/unifap"),
    );
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
            let (_, position) = anchor(
                &unifap_project(),
                UNIFAP_ROUTER,
                "function _safeTransferFrom(",
                "function ",
            );
            assert_eq!(
                analysis.prepare_call_hierarchy(&router, position).unwrap()[0].name,
                "_safeTransferFrom"
            );
        }
        bench(&mut c.benchmark_group("lsp/single-workspace-unchanged"), name, || {
            analysis.run_epoch()
        });
        bench_batched_ref(
            &mut c.benchmark_group("lsp/single-workspace-cold"),
            name,
            prepare,
            |analysis| analysis.run_epoch(),
        );
        bench(&mut c.benchmark_group("lsp/single-workspace-reverted-edit"), name, || {
            analysis.edit_and_revert();
            analysis.run_epoch()
        });
        bench_batched_ref(
            &mut c.benchmark_group("lsp/single-workspace-open-indexed"),
            name,
            || {
                let mut analysis = prepare();
                analysis.clear_open_documents();
                assert!(analysis.run_epoch());
                analysis.assert_no_diagnostics();
                analysis
            },
            |analysis| {
                analysis.replace_source(&main, source);
                analysis.run_epoch()
            },
        );
        let mut edited = false;
        let edited_source = format!("{source} ");
        bench(&mut c.benchmark_group("lsp/single-workspace-changed"), name, || {
            edited = !edited;
            analysis.replace_source(&main, if edited { &edited_source } else { source });
            analysis.run_epoch()
        });
    }
}

fn workspace_index_reuse(c: &mut Criterion) {
    let workspace_count = 4;
    let caller_count = 256;
    let temp = tempfile::tempdir().expect("workspace index benchmark directory");
    let source = callers_source(DEPENDENCY_MAIN, caller_count, "", "uint marker0;\n}\n");
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
    let name = |scenario| format!("4-workspaces-256-callers-per-workspace-{scenario}");

    let mut group = c.benchmark_group("lsp/workspace-index-reuse");
    bench_batched_ref(
        &mut group,
        name("cold"),
        || BenchmarkRepeatedAnalysis::from_workspaces(&roots, &source),
        |analysis| analysis.run_epoch(),
    );
    bench_batched_ref(
        &mut group,
        name("open-indexed-document-first-call-hierarchy"),
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
            analysis.prepare_call_hierarchy(&uri, position)
        },
    );

    let mut analysis = BenchmarkRepeatedAnalysis::from_workspaces(&roots, &source);
    assert!(analysis.run_epoch());
    assert_eq!(analysis.prepare_call_hierarchy(&uri, position).unwrap().len(), 1);
    // Initialization and the first lazy query happen once, outside every incremental sample.
    bench(&mut group, name("unchanged"), || analysis.run_epoch());
    bench(&mut group, name("unchanged-first-call-hierarchy"), || {
        black_box(analysis.run_epoch());
        analysis.prepare_call_hierarchy(&uri, position)
    });
    bench(&mut group, name("reverted-edit-first-call-hierarchy"), || {
        analysis.edit_and_revert();
        black_box(analysis.run_epoch());
        analysis.prepare_call_hierarchy(&uri, position)
    });
    let mut edited = false;
    bench(&mut group, name("one-workspace-edit-first-call-hierarchy"), || {
        edited = !edited;
        analysis.replace_source(&path, if edited { &edited_source } else { &source });
        black_box(analysis.run_epoch());
        analysis.prepare_call_hierarchy(&uri, position)
    });
    group.finish();
}

fn workspace_path_queries(c: &mut Criterion) {
    let queries =
        BenchmarkWorkspacePathQueries::new(PATH_INDEX_WORKSPACE_COUNT, PATH_INDEX_QUERY_COUNT);
    assert_ne!(queries.run(), 0);
    assert_eq!(queries.run(), queries.run_cached());

    let workspaces = format!("{PATH_INDEX_WORKSPACE_COUNT}-workspaces");
    let mut group = c.benchmark_group("lsp/workspace-path-queries");
    group.throughput(Throughput::Elements(PATH_INDEX_QUERY_COUNT as u64));
    let queries_name = format!("{workspaces}-{PATH_INDEX_QUERY_COUNT}-queries");
    bench(&mut group, &queries_name, || queries.run());
    bench(&mut group, format!("{queries_name}-cached"), || queries.run_cached());
    group.finish();

    let mut group = c.benchmark_group("lsp/workspace-path-single-query");
    group.throughput(Throughput::Elements(1));
    bench(&mut group, format!("{workspaces}-single-query"), || queries.run_one());
    bench(&mut group, format!("{workspaces}-single-query-cached"), || queries.run_cached_one());
    group.finish();

    let mut group = c.benchmark_group("lsp/workspace-path-containment-query");
    group.throughput(Throughput::Elements(1));
    bench(&mut group, format!("{workspaces}-containment-query"), || queries.run_containment_one());
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
            let count = |suffix| {
                locations.iter().filter(|location| location.uri.path().ends_with(suffix)).count()
            };
            assert_eq!(locations.len(), 4);
            assert_eq!(count("/src/UnifapV2Factory.sol"), 1);
            assert_eq!(count("/src/test/UnifapV2Factory.t.sol"), 3);
        }
        ("workspace-symbols", BenchmarkResponse::WorkspaceSymbols(symbols)) => {
            assert_eq!(symbols.len(), 3);
            assert!(symbols.iter().all(|symbol| symbol.name.contains("UnifapV2Pair")));
            assert!(symbols.iter().any(|symbol| symbol.name == "UnifapV2Pair"));
        }
        _ => panic!("unexpected `{name}` response for the unifap-v2 benchmark"),
    }
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
    analyze(project.clone());
    let (uri, position) = project
        .unique_anchor(BENCHMARK_FILE, "_addr) internal pure returns (string memory out_)")
        .unwrap();
    let mut requests = BenchmarkRenameRequests::new(project.clone(), uri.clone(), position);
    let response = requests.run().expect("the Predeploys getName argument should be renameable");
    let edits = &response.changes.as_ref().unwrap()[&uri];
    assert_eq!(edits.len(), 31);
    assert!(edits.iter().all(|edit| edit.new_text == "renamed"));
    bench(&mut c.benchmark_group("lsp/rename"), "optimism-predeploys", || requests.run());
    bench_batched(
        &mut c.benchmark_group("lsp/project-analysis"),
        "optimism-predeploys",
        || project.clone(),
        |project| project.analyze(),
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

    let analysis = analyze(project.clone());
    for (name, request) in &requests {
        assert_unifap_response(name, analysis.execute(request));
    }
    let edited = || {
        let mut project = project.clone();
        project.apply_edit(&edit).expect("the benchmark edit should apply");
        project
    };
    edited()
        .unique_anchor(UNIFAP_PAIR, "MINIMUM_LIQUIDITY = 1e4")
        .expect("the edited source should contain the replacement");
    analyze(edited());

    let mut group = c.benchmark_group("lsp/project-analysis");
    bench_batched(&mut group, UNIFAP_PROJECT, || project.clone(), |project| project.analyze());
    group.finish();

    let mut group = c.benchmark_group("lsp/project-analysis-after-edit");
    bench_batched(&mut group, UNIFAP_PROJECT, edited, |project| project.analyze());
    group.finish();

    let mut group = c.benchmark_group("lsp/project-edit-application");
    group.throughput(Throughput::Elements(1));
    bench_batched(&mut group, UNIFAP_PROJECT, || document_change.clone(), |change| change.apply());
    group.finish();

    let mut group = c.benchmark_group("lsp/symbol-table-queries");
    for (name, request) in &requests {
        bench(&mut group, format!("{UNIFAP_PROJECT}-{name}"), || {
            analysis.execute(black_box(request))
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
