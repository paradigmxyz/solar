# Fuzzing #

The fuzzing suite uses [`cargo-fuzz`](https://rust-fuzz.github.io/book/cargo-fuzz.html), the conventional frontend for a variety of fuzzing backends (for now, only `libfuzzer`).

## Installation ##

```
$ cargo install cargo-fuzz
```

## Usage ##

To list each fuzzing case,

```
$ cargo +nightly fuzz list
canonicalize
parsing_context_parse_and_resolve
run_compiler_args
```

In order to run a fuzzing case,

```
$ cargo +nightly fuzz run <NAME>
```

In the event that a panic is encountered a useful stack trace will be necessary. In order to achieve this, you'll need to both specify backtraces the usual way but also instruct the fuzzer to use a debug build of Solar. A useful idiom for this is:

```
$ RUST_BACKTRACE=1 cargo +nightly fuzz run <NAME> --dev
```

Note that this will sadly mean a full recompilation will be incurred.

## Development ##

To add a new fuzzing case,

```
$ cargo +nightly fuzz add <NAME>
```

