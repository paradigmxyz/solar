#![allow(unused_imports)]

use iai_callgrind::{library_benchmark, library_benchmark_group};
use solar_bench::{Parser, Slang, Solang, Solar, Source, get_srcs};
use std::hint::black_box;

#[library_benchmark]
fn solar_enter() -> usize {
    let f: fn() -> usize = || 42usize;
    solar_parse::interface::enter(black_box(f))
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
);

#[inline]
fn run_lex(name: &str, parser: &dyn Parser) {
    assert!(parser.can_lex(), "{} can't lex", parser.name());
    let Source { name: _, path: _, src } = get_source(name);
    solar_parse::interface::enter(|| parser.lex(black_box(src)))
}

#[inline]
fn run_parse(name: &str, parser: &dyn Parser) {
    let Source { name: _, path: _, src } = get_source(name);
    solar_parse::interface::enter(|| parser.parse(black_box(src)))
}

#[inline]
fn get_source(name: &str) -> &'static Source {
    get_srcs().iter().find(|s| s.name == name).unwrap()
}

// use lex_::lex;
// use parse_::parse;

// iai_callgrind::main!(library_benchmark_groups = lex, parse);

library_benchmark_group!(name = all; benchmarks = solar_enter, lex, parse);
iai_callgrind::main!(library_benchmark_groups = all);
