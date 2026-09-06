#![allow(unused_crate_dependencies)]

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use crop::Rope;
use lsp_types::{GotoDefinitionResponse, HoverContents, OneOf, Position, Url};
use solar_config::CompileOpts;
use solar_lsp::{
    BenchmarkAnalysis, BenchmarkDocumentUpdate, BenchmarkFoldingRangeRequests,
    BenchmarkOpenDocuments, BenchmarkProject, BenchmarkRepeatedAnalysis, BenchmarkRequest,
    BenchmarkResponse, BenchmarkSelectionRangeRequests, BenchmarkWorkspaceDiscovery,
    BenchmarkWorkspacePathQueries, BenchmarkWorkspaceReports, benchmark_folding_ranges,
    benchmark_folding_ranges_from_rope, benchmark_selection_ranges,
};
use std::{fs, hint::black_box, path::PathBuf};

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

struct SourceBuilder {
    source: String,
    hover_anchors: Vec<String>,
}

impl SourceBuilder {
    fn new(function_count: usize) -> Self {
        Self { source: String::new(), hover_anchors: Vec::with_capacity(function_count * 2) }
    }

    fn push_line(&mut self, line: &str) {
        self.source.push_str(line);
        self.source.push('\n');
    }

    fn push_hover_anchor(&mut self, anchor: String) {
        self.hover_anchors.push(anchor);
    }

    fn finish(self) -> BenchmarkSource {
        let project = BenchmarkProject::from_source(self.source.clone());
        let hover_positions = self
            .hover_anchors
            .into_iter()
            .map(|anchor| {
                let (_, position) = project
                    .unique_anchor("benchmark.sol", &anchor)
                    .expect("generated hover anchors should be unique");
                (position.line, position.character)
            })
            .collect();
        BenchmarkSource { source: self.source, project, hover_positions }
    }
}

fn benchmark_source(function_count: usize) -> BenchmarkSource {
    let mut builder = SourceBuilder::new(function_count);
    builder.push_line("contract Benchmark {");
    for index in 0..function_count {
        let name = format!("function_{index:04}");
        builder.push_line(&format!(
            "    /// @notice Processes values for benchmark function {index}."
        ));
        builder.push_line("    /// @dev Used to measure resolved NatSpec rendering.");
        builder.push_line("    /// @param first The first input value.");
        builder.push_line("    /// @param second The second input value.");
        builder.push_line("    /// @param account The account returned by the function.");
        builder.push_line("    /// @return total The sum of both input values.");
        builder.push_line("    /// @return owner The supplied account.");
        let declaration = format!(
            "    function {name}(uint256 first, uint256 second, address account) public pure returns (uint256 total, address owner) {{"
        );
        builder.push_line(&declaration);
        builder.push_hover_anchor(format!("{name}(uint256 first"));
        builder.push_line("        total = first + second;");
        builder.push_line("        owner = account;");
        builder.push_line("    }");
    }

    builder.push_line("    function exercise() public pure {");
    for index in 0..function_count {
        let name = format!("function_{index:04}");
        let call = format!("        {name}(1, 2, address(0));");
        builder.push_line(&call);
        builder.push_hover_anchor(format!("{name}(1, 2, address(0))"));
    }
    builder.push_line("    }");
    builder.push_line("}");
    builder.finish()
}

fn analysis_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/analysis-build");
    for function_count in ANALYSIS_FUNCTION_COUNTS {
        let fixture = benchmark_source(function_count);
        let analysis = BenchmarkAnalysis::from_source(fixture.source.clone());
        assert_clean(&analysis);
        group.throughput(Throughput::Bytes(fixture.source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(function_count),
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
    group.finish();
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
    // Manifest discovery prunes the import-only root without visiting its 10,000 descendants.
    // The full workspace load still scans it once for remappings; discovery must not repeat it.
    assert_eq!(baseline.pruned(), 1);
    assert_eq!(baseline.visited(), 4);

    let mut group = c.benchmark_group("lsp/workspace-discovery");
    group.bench_function("foundry-10k-import-only", |b| {
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
            BenchmarkId::from_parameter(batch_count),
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

    let end_to_end_id = format!("lsp/project-analysis-batched/{AGGREGATION_BATCH_COUNT}");
    c.bench_function(&end_to_end_id, |b| {
        b.iter_batched(
            || project.clone(),
            |project| {
                black_box(BenchmarkAnalysis::merge(black_box(project.analyze_file_batches())))
            },
            BatchSize::PerIteration,
        );
    });
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
    group.bench_function(HOVER_FUNCTION_COUNT.to_string(), |b| {
        b.iter(|| {
            let analysis = black_box(&analysis);
            for &(line, character) in black_box(&positions) {
                black_box(analysis.hover(black_box(line), black_box(character)));
            }
        });
    });
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
    group.bench_function("optimism", |b| {
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
    for (name, source) in [("clean", clean), ("incomplete", incomplete), ("minified", minified)] {
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
    group.bench_function("open-clean-legacy", |b| {
        b.iter_batched(
            || open_rope.clone(),
            |rope| {
                let source = rope_to_string(&rope);
                black_box(benchmark_folding_ranges(black_box(source)))
            },
            BatchSize::PerIteration,
        );
    });
    group.bench_function("open-clean-snapshot", |b| {
        b.iter_batched(
            || open_rope.clone(),
            |rope| black_box(benchmark_folding_ranges_from_rope(black_box(rope))),
            BatchSize::PerIteration,
        );
    });
    group.finish();

    let requests = BenchmarkFoldingRangeRequests::new(OPTIMISM_SOURCE.to_owned());
    assert_eq!(requests.run(), clean_ranges);
    let mut cached = c.benchmark_group("lsp/open-document-folding-range");
    cached.throughput(Throughput::Bytes(OPTIMISM_SOURCE.len() as u64));
    cached.bench_function("optimism-unchanged", |b| {
        b.iter(|| black_box(requests.run()));
    });
    cached.finish();
}

fn open_document_selection_range(c: &mut Criterion) {
    let positions = [Position::new(0, 0)];
    let requests =
        BenchmarkSelectionRangeRequests::new(OPTIMISM_SOURCE.to_owned(), positions.iter().copied());
    let expected = benchmark_selection_ranges(OPTIMISM_SOURCE.to_owned(), &positions)
        .expect("the benchmark position should be valid");
    let ranges = requests.run().expect("the benchmark position should be valid");
    assert_eq!(ranges, expected);
    assert_eq!(ranges.len(), positions.len());
    assert!(ranges[0].range.start <= positions[0] && positions[0] < ranges[0].range.end);

    let mut group = c.benchmark_group("lsp/open-document-selection-range");
    group.throughput(Throughput::Bytes(OPTIMISM_SOURCE.len() as u64));
    group.bench_function("optimism", |b| {
        b.iter(|| black_box(black_box(&requests).run()));
    });
    group.finish();
}

fn workspace_diagnostic_hot_paths(c: &mut Criterion) {
    let source = OPTIMISM_SOURCE.to_owned();
    assert_eq!(BenchmarkDocumentUpdate::from_source(source.clone()).apply(), 1);
    let mut updates = c.benchmark_group("lsp/unchanged-document-update");
    updates.throughput(Throughput::Bytes(source.len() as u64));
    updates.bench_function("optimism", |b| {
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
            BenchmarkId::from_parameter(document_count),
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

fn open_document_analysis_batches(c: &mut Criterion) {
    let documents = BenchmarkOpenDocuments::new(OPEN_DOCUMENT_COUNT, OPEN_DOCUMENT_BYTES);
    // Prime the initial snapshot so timing measures reuse across later analysis epochs.
    assert!(documents.build_analysis_batches() >= documents.source_bytes());

    let mut group = c.benchmark_group("lsp/open-document-analysis-batches");
    group.throughput(Throughput::Bytes(documents.source_bytes() as u64));
    group.bench_function(format!("{OPEN_DOCUMENT_COUNT}x{OPEN_DOCUMENT_BYTES}"), |b| {
        b.iter(|| black_box(documents.build_analysis_batches()))
    });
    group.finish();
}

fn repeated_analysis(c: &mut Criterion) {
    let fixture = benchmark_source(256);

    let mut cold = c.benchmark_group("lsp/incremental-analysis");
    cold.bench_function("cold", |b| {
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
    cached.bench_function("unchanged", |b| b.iter(|| black_box(analysis.run())));
    cached.finish();
}

fn workspace_path_queries(c: &mut Criterion) {
    let queries =
        BenchmarkWorkspacePathQueries::new(PATH_INDEX_WORKSPACE_COUNT, PATH_INDEX_QUERY_COUNT);
    assert_ne!(queries.run(), 0);

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
    group.bench_function(UNIFAP_PROJECT, |b| {
        b.iter_batched(
            || project.clone(),
            |project| black_box(project.analyze()),
            BatchSize::PerIteration,
        );
    });
    group.finish();

    let mut group = c.benchmark_group("lsp/project-analysis-after-edit");
    group.bench_function(UNIFAP_PROJECT, |b| {
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
    group.bench_function(UNIFAP_PROJECT, |b| {
        b.iter_batched(
            || document_change.clone(),
            |change| black_box(change.apply()),
            BatchSize::PerIteration,
        );
    });
    group.finish();

    let mut group = c.benchmark_group("lsp/symbol-table-queries");
    for (name, request) in &requests {
        let id = format!("{UNIFAP_PROJECT}/{name}");
        group.bench_with_input(id, request, |b, request| {
            b.iter(|| black_box(analysis.execute(black_box(request))))
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    analysis_build,
    bounded_workspace_discovery,
    symbol_table_aggregation,
    burst_hover,
    folding_range,
    selection_range,
    open_document_selection_range,
    workspace_diagnostic_hot_paths,
    open_document_analysis_batches,
    repeated_analysis,
    workspace_path_queries,
    unifap_benches
);
criterion_main!(benches);
