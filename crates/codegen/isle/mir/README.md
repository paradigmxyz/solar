# MIR rule modules

The build script lists every module explicitly. Each directory is one logical
rule set; files share declarations and compile together, so ISLE checks overlaps
across module boundaries.

- `egraph/`: value simplifications and in-place alternatives. `prelude.isle`
  declares shared terms and Rust-backed helpers.
- `word/`: pure word alternatives, compiled with the e-graph modules.
- `word_sequence/`: bounded recipes selected after e-graph extraction, with its
  own `prelude.isle` for recipe construction.

Within each directory, rules are grouped by the root operation: arithmetic,
bitwise, comparisons, shifts, and selects. E-graph memory/environment rules live
in `memory.isle`; cast identities live in `types.isle`, including comparisons
that cancel matching casts. Boolean AND/OR/XOR rules belong in `bitwise.isle`.
Keep rules for the same operation in their existing order, preserve priorities,
and add new modules to `crates/codegen/build.rs`.

The offline checker accepts either a module file or a rule-set directory. A
directory selects every immediate `*.isle` file, including any newly added
module. Declarations have no proof obligations. Shards divide the rules across
the whole directory, and reports retain each rule's module and line number.
