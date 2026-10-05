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

## Matching and costs

`rewrite` supplies equivalent instructions for target-cost selection; `simplify`
merges an instruction with an existing value. Higher `simplify` priorities run
first, with zero as the default. Priorities set match order, not cost. The
`multi` rewrite terms collect all matching alternatives.

Matching resolves value identities and puts constants on the right of declared
commutative pairs and comparisons, including nested definitions. Nonconstant
operand permutations still need separate rules. Word alternatives stay at the
original instruction and pay target costs plus stack traffic. An `Add(value, 0)`
replacement represents a leaf through the instruction-valued interface; ordinary
simplification removes the add. Recipes create temporary instructions only after
cost selection.

## Comment notation

Each rule has a `before => after` description followed by any required guards.
Use infix arithmetic and bitwise operators; use lowercase function calls for
operations without an infix spelling. Describe complete replacement expressions,
not the names of temporary recipe nodes. Keep operand-order variants accurate.

Word arithmetic wraps at 256 bits. `/`, `%`, and comparisons are unsigned;
`sdiv`, `smod`, and `signed(x)` select signed semantics. EVM division and modulo
by zero yield zero. `<<` and `>>` are saturating logical shifts; `sar(count, x)`
is an arithmetic shift. Counts and combined shift counts never wrap modulo 256. `byte(index, x)` counts
from the high byte, and `**` denotes exponentiation.

`MAX` is `2 ** 256 - 1`; `MIN_SIGNED` and `MAX_SIGNED` are the words `2 ** 255`
and `2 ** 255 - 1`. Booleans are 0 or 1, and `select(c, a, b)` chooses `a` for
nonzero `c`. `x:iN` marks an N-bit integer. Width guards require integer sources of at most
256 bits; other widths follow the matched MIR types. Constants and their arithmetic fold at compile time. Type, range,
fork, and profitability guards remain explicit beside the rules. Cast rewrites
also obey the pass's type checks before values merge.
