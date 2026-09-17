//! Name every case `lsp/<operation>[<scenario>]` using parameter IDs so CodSpeed retains the
//! operation. Include the corpus or workload size with units, plus any cache or cursor state.

#![allow(unused_crate_dependencies)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use lsp_types::CodeActionOrCommand;
use solar_lsp::{BenchmarkCodeActionRequests, benchmark_diagnostic_conversion};
use std::{fmt::Write as _, hint::black_box};

const OPTIMISM_SOURCE: &str = include_str!("../../../testdata/Optimism.sol");

fn diagnostic_conversion(c: &mut Criterion) {
    let source = OPTIMISM_SOURCE.to_owned();
    let mut group = c.benchmark_group("lsp/diagnostic-conversion");
    group.throughput(Throughput::Bytes(source.len() as u64));
    for diagnostic_count in [1, 16, 64] {
        for cached in [false, true] {
            let name = format!(
                "optimism-{diagnostic_count}-diagnostics-{}",
                if cached { "cached" } else { "uncached" }
            );
            assert_eq!(
                benchmark_diagnostic_conversion(source.clone(), diagnostic_count, cached),
                diagnostic_count
            );
            group.bench_function(BenchmarkId::from_parameter(name), |b| {
                b.iter(|| {
                    black_box(benchmark_diagnostic_conversion(
                        black_box(source.clone()),
                        diagnostic_count,
                        cached,
                    ))
                });
            });
        }
    }
    group.finish();
}

fn code_actions(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/code-actions");
    for function_count in [1, 64, 256] {
        let mut source = String::from(
            "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Actions {\n",
        );
        for index in 0..function_count {
            writeln!(
                source,
                "function value{index}() public returns (uint256) {{ return {index}; }}"
            )
            .unwrap();
        }
        source.push_str("}\n");
        for whole_document in [false, true] {
            let mut requests = BenchmarkCodeActionRequests::new(source.clone(), whole_document);
            let actions = requests.run();
            assert_eq!(actions.len(), if whole_document { function_count } else { 1 });
            for action in &actions {
                let CodeActionOrCommand::CodeAction(action) = action else {
                    panic!("expected a literal quick fix");
                };
                assert_eq!(action.title, "Change state mutability to `pure`");
                assert!(action.edit.is_some());
            }
            group.bench_function(
                BenchmarkId::from_parameter(format!(
                    "{function_count}-functions-{}-quick-fixes",
                    if whole_document { "whole-document" } else { "cursor" },
                )),
                |b| {
                    b.iter(|| black_box(requests.run()));
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, diagnostic_conversion, code_actions);
criterion_main!(benches);
