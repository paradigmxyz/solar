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

## Development ##

To add a new fuzzing case,

```
$ cargo +nightly fuzz add <NAME>
```

