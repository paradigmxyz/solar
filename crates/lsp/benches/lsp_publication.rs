#![allow(unused_crate_dependencies)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use lsp_types::{DiagnosticSeverity, NumberOrString};
use solar_lsp::BenchmarkRepeatedAnalysis;
use std::{fmt::Write as _, hint::black_box};

fn warning_source(function_count: usize) -> String {
    let mut source = String::from(
        "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract Warnings {\n",
    );
    for index in 0..function_count {
        writeln!(source, "function value{index}() public returns (uint256) {{ return {index}; }}")
            .unwrap();
    }
    source.push_str("}\n");
    source
}

fn cached_diagnostic_publication(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp/cached-diagnostic-publication");
    for function_count in [0, 1, 64, 256, 1024] {
        let source = warning_source(function_count);

        group.throughput(Throughput::Bytes(source.len() as u64));
        for pull in [false, true] {
            let mut analysis =
                BenchmarkRepeatedAnalysis::with_diagnostic_delivery(source.clone(), pull);
            assert!(analysis.run_epoch());
            let expected = analysis.diagnostic_reports();
            assert_eq!(expected.len(), 1);
            assert_eq!(expected[0].2.len(), function_count);
            for diagnostic in &expected[0].2 {
                assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
                assert_eq!(diagnostic.code, Some(NumberOrString::String("2018".into())));
            }
            assert!(analysis.run_epoch());
            assert_eq!(analysis.diagnostic_reports(), expected);

            // The cached production analysis path includes report replacement and publication.
            // The closed client excludes transport writes while retaining notification creation.
            group.bench_function(
                BenchmarkId::new(if pull { "pull" } else { "push" }, function_count),
                |b| b.iter(|| black_box(analysis.run_epoch())),
            );
            assert_eq!(analysis.diagnostic_reports(), expected);
        }
    }
    group.finish();
}

fn reverted_edit_diagnostic_publication(c: &mut Criterion) {
    let source = warning_source(256);
    let mut group = c.benchmark_group("lsp/reverted-edit-diagnostic-publication");
    for pull in [false, true] {
        let mut analysis =
            BenchmarkRepeatedAnalysis::with_diagnostic_delivery(source.clone(), pull);
        assert!(analysis.run_epoch());
        let expected = analysis.diagnostic_reports();
        assert_eq!(expected.len(), 1);
        assert_eq!(expected[0].2.len(), 256);
        analysis.edit_and_revert();
        assert!(analysis.run_epoch());
        assert_eq!(analysis.diagnostic_reports(), expected);
        group.bench_function(if pull { "pull/256" } else { "push/256" }, |b| {
            b.iter(|| {
                analysis.edit_and_revert();
                black_box(analysis.run_epoch())
            });
        });
        assert_eq!(analysis.diagnostic_reports(), expected);
    }
    group.finish();
}

criterion_group!(benches, cached_diagnostic_publication, reverted_edit_diagnostic_publication);
criterion_main!(benches);
