#![allow(unused_crate_dependencies)]

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use lsp_types::{GotoDefinitionResponse, HoverContents};
use solar_lsp::{BenchmarkAnalysis, BenchmarkProject, BenchmarkRequest, BenchmarkResponse};
use std::{hint::black_box, path::PathBuf};

const ANALYSIS_FUNCTION_COUNTS: [usize; 2] = [64, 256];
const HOVER_FUNCTION_COUNT: usize = 256;
const UNIFAP_PROJECT: &str = "unifap-v2";
const UNIFAP_ROUTER: &str = "src/UnifapV2Router.sol";
const UNIFAP_PAIR: &str = "src/UnifapV2Pair.sol";
const UNIFAP_FACTORY: &str = "src/UnifapV2Factory.sol";

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

    let mut group = c.benchmark_group("lsp/symbol-table-queries/unifap-v2");
    group.throughput(Throughput::Elements(1));
    for (name, request) in &requests {
        group.bench_with_input(BenchmarkId::from_parameter(name), request, |b, request| {
            b.iter(|| black_box(analysis.execute(black_box(request))));
        });
    }
    group.finish();
}

criterion_group!(benches, analysis_build, burst_hover, unifap_benches);
criterion_main!(benches);
