#![allow(unused_crate_dependencies)]

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use solar_lsp::BenchmarkAnalysis;
use std::hint::black_box;

const ANALYSIS_FUNCTION_COUNTS: [usize; 2] = [64, 256];
const HOVER_FUNCTION_COUNT: usize = 256;

struct BenchmarkSource {
    source: String,
    hover_positions: Vec<(u32, u32)>,
}

struct SourceBuilder {
    source: String,
    hover_positions: Vec<(u32, u32)>,
    next_line: u32,
}

impl SourceBuilder {
    fn new(function_count: usize) -> Self {
        Self {
            source: String::new(),
            hover_positions: Vec::with_capacity(function_count * 2),
            next_line: 0,
        }
    }

    fn push_line(&mut self, line: &str) -> u32 {
        let line_number = self.next_line;
        self.source.push_str(line);
        self.source.push('\n');
        self.next_line += 1;
        line_number
    }

    fn push_hover_position(&mut self, line: u32, source_line: &str, name: &str) {
        let character = source_line.find(name).expect("benchmark symbol should be present") as u32;
        self.hover_positions.push((line, character));
    }

    fn finish(self) -> BenchmarkSource {
        BenchmarkSource { source: self.source, hover_positions: self.hover_positions }
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
        let line = builder.push_line(&declaration);
        builder.push_hover_position(line, &declaration, &name);
        builder.push_line("        total = first + second;");
        builder.push_line("        owner = account;");
        builder.push_line("    }");
    }

    builder.push_line("    function exercise() public pure {");
    for index in 0..function_count {
        let name = format!("function_{index:04}");
        let call = format!("        {name}(1, 2, address(0));");
        let line = builder.push_line(&call);
        builder.push_hover_position(line, &call, &name);
    }
    builder.push_line("    }");
    builder.push_line("}");
    builder.finish()
}

fn analysis_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/analysis-build");
    for function_count in ANALYSIS_FUNCTION_COUNTS {
        let fixture = benchmark_source(function_count);
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
    let analysis = BenchmarkAnalysis::from_source(fixture.source);
    let positions = fixture.hover_positions;
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

criterion_group!(benches, analysis_build, burst_hover);
criterion_main!(benches);
