#![allow(unused_imports)]

use gungraun::{library_benchmark, library_benchmark_group};
use solar_bench::{Compiler, Slang, Solang, Solar, Source, get_src};
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
        fn lex(name: &str, compiler: &dyn Compiler) {
            run_lex(name, compiler);
        }

        #[library_benchmark]
        #[benches::parse(
            $(
                ($name, &Solar),
                ($name, &Solang),
                ($name, &Slang),
            )*
        )]
        fn parse(name: &str, compiler: &dyn Compiler) {
            run_parse(name, compiler);
        }

        /*
        mod lex_ {
            use super::*;

            $(
                #[library_benchmark]
                #[benches::$name(Solar, Solang)]
                fn $name(compiler: impl Compiler) {
                    run_lex(stringify!($name), compiler);
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
                fn $name(compiler: impl Compiler) {
                    run_parse(stringify!($name), compiler);
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

#[inline]
fn run_lex(name: &str, compiler: &dyn Compiler) {
    assert!(compiler.capabilities().can_lex(), "{} can't lex", compiler.name());
    let source = get_src(name);
    let setup = &mut *compiler.setup(source);
    compiler.lex(black_box(source), setup)
}

#[inline]
fn run_parse(name: &str, compiler: &dyn Compiler) {
    let source = get_src(name);
    let setup = &mut *compiler.setup(source);
    compiler.parse(black_box(source), setup)
}

// use lex_::lex;
// use parse_::parse;

// gungraun::main!(library_benchmark_groups = lex, parse);

library_benchmark_group!(name = all; benchmarks = solar_enter, lex, parse);
gungraun::main!(library_benchmark_groups = all);
