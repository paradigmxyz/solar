#![allow(unused_imports)]

use iai_callgrind::{library_benchmark, library_benchmark_group};
use solar_bench::{Parser, Slang, Solang, Solar, Source, get_src};
use std::hint::black_box;

#[library_benchmark]
fn solar_enter() -> usize {
    let f: fn() -> usize = || 42usize;
    solar::parse::interface::enter(black_box(f))
}

#[cfg(feature = "ci")]
macro_rules! mk_groups {
    ($($name:literal),* $(,)?) => {
        #[library_benchmark]
        #[benches::ci_lex($($name),*)]
        fn lex(name: &str) {
            run_lex(name, &Solar);
        }

        #[library_benchmark]
        #[benches::ci_parse($($name),*)]
        fn parse(name: &str) {
            run_parse(name, &Solar);
        }
    }
}

#[cfg(not(feature = "ci"))]
macro_rules! mk_groups {
    ($($name:literal),* $(,)?) => {
        #[library_benchmark]
        #[benches::lex(
            $(
                ($name, &Solar),
                ($name, &Solang),
            )*
        )]
        fn lex(name: &str, parser: &dyn Parser) {
            run_lex(name, parser);
        }

        #[library_benchmark]
        #[benches::parse(
            $(
                ($name, &Solar),
                ($name, &Solang),
                ($name, &Slang),
            )*
        )]
        fn parse(name: &str, parser: &dyn Parser) {
            run_parse(name, parser);
        }

        /*
        mod lex_ {
            use super::*;

            $(
                #[library_benchmark]
                #[benches::$name(Solar, Solang)]
                fn $name(parser: impl Parser) {
                    run_lex(stringify!($name), parser);
                }
            )*

            library_benchmark_group!(
                name = lex;
                benchmarks = $($name,)*
            );
        }

        mod parse_ {
            use super::*;

            $(
                #[library_benchmark]
                #[benches::$name(Solar, Solang, Slang)]
                fn $name(parser: impl Parser) {
                    run_parse(stringify!($name), parser);
                }
            )*

            library_benchmark_group!(
                name = parse;
                benchmarks = $($name,)*
            );
        }
        */
    };
}

mk_groups!(
    "empty",
    "Counter",
    "verifier",
    "OptimizorClub",
    "UniswapV3",
    "Solarray",
    "console",
    "Vm",
    "safeconsole",
    "Seaport",
    "Solady",
    "Optimism",
);

#[cfg(feature = "ci")]
#[library_benchmark]
#[benches::ci_analyze(
    "empty",
    "Counter",
    "verifier",
    "OptimizorClub",
    "UniswapV3",
    "Solarray",
    "console",
    "Vm",
    "safeconsole",
    "Seaport",
    "Solady"
)]
fn analyze(name: &str) {
    run_analyze(name, &Solar);
}

#[cfg(feature = "ci")]
#[library_benchmark]
#[benches::ci_analyze_no_drop(
    "empty",
    "Counter",
    "verifier",
    "OptimizorClub",
    "UniswapV3",
    "Solarray",
    "console",
    "Vm",
    "safeconsole",
    "Seaport",
    "Solady"
)]
fn analyze_no_drop(name: &str) {
    run_analyze_no_drop(name, &Solar);
}

#[cfg(not(feature = "ci"))]
#[library_benchmark]
#[benches::analyze(
    "empty",
    "Counter",
    "verifier",
    "OptimizorClub",
    "UniswapV3",
    "Solarray",
    "console",
    "Vm",
    "safeconsole",
    "Seaport",
    "Solady"
)]
fn analyze(name: &str) {
    run_analyze(name, &Solar);
}

#[cfg(not(feature = "ci"))]
#[library_benchmark]
#[benches::analyze_no_drop(
    "empty",
    "Counter",
    "verifier",
    "OptimizorClub",
    "UniswapV3",
    "Solarray",
    "console",
    "Vm",
    "safeconsole",
    "Seaport",
    "Solady"
)]
fn analyze_no_drop(name: &str) {
    run_analyze_no_drop(name, &Solar);
}

#[inline]
fn run_lex(name: &str, parser: &dyn Parser) {
    assert!(parser.capabilities().can_lex(), "{} can't lex", parser.name());
    let Source { name: _, path: _, src, capabilities: _ } = get_src(name);
    let setup = &mut *parser.setup(src);
    parser.lex(black_box(src), setup)
}

#[inline]
fn run_parse(name: &str, parser: &dyn Parser) {
    let Source { name: _, path: _, src, capabilities: _ } = get_src(name);
    let setup = &mut *parser.setup(src);
    parser.parse(black_box(src), setup)
}

#[inline]
fn run_analyze(name: &str, parser: &dyn Parser) {
    let Source { name: _, path: _, src, capabilities } = get_src(name);
    assert!(parser.capabilities().can_analyze(), "{} can't analyze", parser.name());
    assert!(capabilities.can_analyze(), "{name} can't be analyzed");
    let setup = &mut *parser.setup(src);
    parser.analyze(black_box(src), setup)
}

#[inline]
fn run_analyze_no_drop(name: &str, parser: &dyn Parser) {
    let Source { name: _, path: _, src, capabilities } = get_src(name);
    assert!(parser.capabilities().can_analyze(), "{} can't analyze", parser.name());
    assert!(capabilities.can_analyze(), "{name} can't be analyzed");
    let setup = &mut *parser.setup(src);
    parser.analyze_no_drop(black_box(src), setup)
}

// use lex_::lex;
// use parse_::parse;

// iai_callgrind::main!(library_benchmark_groups = lex, parse);

library_benchmark_group!(name = all; benchmarks = solar_enter, lex, parse, analyze, analyze_no_drop);
iai_callgrind::main!(library_benchmark_groups = all);
