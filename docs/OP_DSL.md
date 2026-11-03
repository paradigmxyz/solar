# Operation definition language

Every MIR operation and every EVM opcode is declared in `.ops` files, with every property the
compiler reads from it. Build scripts compile these files with `crates/op-dsl` into Rust that
the IR crates include:

| Files | Declares | Generated into |
| --- | --- | --- |
| `crates/mir/ops/vocabulary.ops` | operand and attribute types, host enums, traits, phase categories, metadata flags, semantics forms | `mir_ops.rs`, included by `op_schema.rs` |
| `crates/mir/ops/core.ops` | 108 operations and 6 classes | `mir_ops.rs` |
| `crates/evmir/ops/vocabulary.ops` | host enums, opcode traits, gas constants and tiers, word functions, logical stack operations | `evm_ops.rs` and `evm_gas.rs`, included by `op.rs` and `gas.rs` |
| `crates/evmir/ops/opcodes.ops` | 89 opcode rows, four of them families | `evm_ops.rs` |

The generated files live in `$OUT_DIR` and are not committed. The ISLE preludes generated from
them (`isle/mir/prelude.isle`, `isle/mir/extractors.isle`, `isle/evm-ir/prelude.isle`) stay
committed and snapshot-tested, so rule authors can read them.

A dialect's vocabulary is declared in the same language as its operations. The generator knows
the shape of each property (a trait is a flag, `effect` names an `EffectKind` variant), not the
list of traits, effects, or categories, so another schema file can use the whole vocabulary,
and an extension can add to it.

## Syntax

A file is a list of nodes. A node is a run of tokens ended by `;` or by a `{ ... }` body of
child nodes. `//` starts a comment; `///` lines document the node that follows and become Rust
doc comments. The first node of every file names its dialect:

```text
dialect mir;          // core MIR schema
dialect mir.acme;     // MIR extension in namespace `acme`
dialect evm;
```

Diagnostics point at the file, line, and column, with a caret under the offending token.

## MIR

### Vocabulary

```text
/// One value operand.
operand value one = "ValueId" isle Value;
operand value[] many = "Box<[ValueId]>";

attribute u32;
attribute Bytes opaque;
attribute PtrLocation: Memory | Calldata | Returndata | Storage | Transient;
attribute AllocationSemantics {
    failure: AllocationFailure;
}

enum AllocationFailure: Infallible | Checked;
enum EffectKind: Pure | MemoryRead | MemoryWrite | ...;

/// The operation may revert.
trait may_revert;

category CheckedArithmetic = "checked arithmetic";
metadata deferred_alloc;
semantics low_bits(value, u32);
```

- `operand NAME one|optional|many = "Rust" [isle T]` declares a type that holds value
  operands. Its Rust type implements `Operands` by hand.
- `attribute NAME [= "Rust"] [isle T | opaque] [: variants] [{ field: Enum; ... }]` declares
  a type that holds no operands. The generator writes its `Operands` impl. Variants after `:`
  are the ones predicates may name; fields in braces are the ones predicates may read with a
  path, as in `semantics.failure`.
- `enum NAME: variants` declares a host enum that properties name. A variant list without a
  trailing `..` must list every Rust variant, and generated code checks that it does.
- `trait`, `category`, and `metadata` add a flag to `OpTraits` (at most 32), a variant to
  `PhaseCategory`, and a metadata flag that phase-legality clauses may test.
- `semantics NAME(types)` declares a form of `Semantics` with the field types of its
  arguments.

### Operations and classes

```text
/// A wrapping integer operation on two operands of one width.
class IntBinary(a: value, b: value) -> Integer {
    traits egraph_rewrite;
    types a: int else i256, b: type(a);
}

/// Addition: `a + b`
op Add: IntBinary {
    traits rematerializable;
    commutative a, b;
    builder;
    semantics opcode(add);
}
```

An operation has tuple fields `Name(a: T, ...)` or struct fields written as `field x: T;`
children, a result kind after `->`, and properties. A class holds shared fields and
properties; `op X: Class` inherits them. Traits accumulate along the chain; any other property
set on the operation replaces the class's. The order of operations in the files is the order of
the `InstKind` variants.

| Property | Meaning | Default |
| --- | --- | --- |
| `mnemonic "text"` | Text-format name | the lowercase name |
| `isle name` | Term name in the ISLE view | the lowercase name |
| `until Phase` | Last phase that admits the operation | every phase |
| `category C` | Diagnostic family when a phase rejects it | none |
| `effect E` | `EffectKind` variant | `Pure` (required in extensions) |
| `side_effects` | Dead-code elimination must keep it | no |
| `traits a, b [if pred]` | `OpTraits` flags, optionally conditional | none |
| `commutative x, y` | The operand pair may be exchanged; sets the trait | no |
| `memory read\|write\|partial_write(addr, size) [if pred]` | Raw memory footprint | none |
| `types f: T, ...` / `types delegated` | Operand type contract | required |
| `semantics form(args)` | What the operation computes | none |
| `builder [name] [void]` | Generates a `FunctionBuilder` method | none |
| `when pred { ... }` | Overrides for matching instructions | none |

`when` clauses refine `mnemonic`, `effect`, `until`, `category`, and `semantics` for the
instructions they match; the first matching clause wins. A clause that tests instruction
metadata (`metadata.deferred_alloc`) may set only `until` and `category`, and the phase
verifier evaluates it. Conditional traits are orthogonal to clauses: every `traits ... if`
line whose condition holds adds its traits, so `InstKind::traits` is the definition's traits
plus the conditional ones.

```text
op ICall -> Custom {
    field function: Callee;
    field args: value[];
    effect ICall;
    side_effects;
    traits may_revert, may_diverge, may_terminate, changes_account_state if function is Function(_);
    traits observes_gas if function is Builtin(Sha256 | Ripemd160 | EcRecover);
    types args: from(function);
    semantics call(function, args);
    when function is Builtin(Check {..}) {
        until Semantic;
        effect Pure;
        category ConditionalCheck;
    }
    when function is Builtin(_) {
        until Semantic;
        effect ExternalCall;
        category Builtin;
    }
}
```

Predicates combine `!`, `&&`, `||`, and parentheses over these atoms:

- `field is Pattern | Pattern`, with nested payload patterns such as `Builtin(Require(_))`;
- `path is Pattern`, a field of a struct attribute, such as `semantics.failure is Checked`;
- `field is absent`, for an optional value operand;
- `field`, for a `bool` attribute;
- `field < 255` and the other integer comparisons;
- `field == CONST`, an associated constant of the field's type (metadata clauses only);
- `metadata.flag`.

Conditions of `traits`, `memory`, `when` (outside metadata clauses), and `types` compile to
`const fn` code, so `InstKind::op_def`, `traits`, and `effects` stay `const`.

The type language describes each operand's expected type: `i1`, `i160`, `i256`,
`ptr<memory>`, `slice<calldata>`, `ptr<#field>` (the location a field names), `int else T` and
`ptr<a | b> else T` (the operand's own type if it matches, else `T`), `type(f)` (an earlier
operand's type, or the type an attribute denotes), `if pred then T else T`, and `from(f)`,
where the field's Rust type computes the types of a variable operand list.

### Traits

`rematerializable`, `commutative`, `egraph_rewrite`, `reversible`, and `shift` steer rewrites
and the backend. The control and state traits replace what `effects.rs` used to list by hand:
`may_revert`, `may_diverge`, `may_terminate`, `observes_gas`, `observes_execution`,
`reads_account_state`, `changes_account_state`, `expands_memory`, `idempotent_write`, and
`fresh_object`. `InstKind::effects` derives elimination, value numbering, and code motion facts
from these traits and from the meaning of each `EffectKind`, which applies to every operation
alike.

## EVM

```text
gas_tier Exp {
    static 10;
    dynamic 10, SpuriousDragon: 50 per byte_len of 1;
}

opcode LT = 0x10 (a, b) -> (r: 1) { traits pure; gas VeryLow; swapped GT; eval lt(a, b); }
opcode SHL = 0x1b (shift, value) -> (r) { traits pure; gas VeryLow; available since Constantinople; eval shl(shift, value); }
/// `PUSHn` pushes its `n`-byte immediate.
opcode PUSH1..=PUSH32 = 0x60 { stack 0 -> 1; gas VeryLow; immediate n; }
```

An opcode row names its stack inputs in pop order, each with the low bits it reads, and its
result with the bits that can be nonzero. Its body sets `traits`, a `gas` tier, `available
since Fork` or a named gate, the `swapped` mirror of a comparison, the `immediate` width, the
`eval` word function of a pure opcode; the mnemonic is the name in lowercase. A family row such
as `DUP1..=DUP16` writes these as expressions in its suffix `n`. `OpcodeTraits` holds at most
16 traits.

- `gas_tier Name[(param: u8)]` gives the static price with the forks that change it, the
  price of a warm access since Berlin, and the dynamic charge per byte, word, or byte length
  of a stack argument.
- `const NAME = value;` declares a gas constant that tiers and the cost model share.
- `word_fn name(args);` declares a primitive of `op/word.rs` that `eval` may call.
- `stack_op Variant { mnemonic; opcode; gas; }` declares a logical stack operation.

The generator emits the opcode constants, the definition table, `from_mnemonic`, `eval_opcode`,
`StackOp::definition`, and the whole `GasTier` schedule.

## What stays in Rust

Each property the language cannot state is a named hook with a typed Rust signature, not a
string of Rust code inside the schema:

| Hook | Users | Why |
| --- | --- | --- |
| `types delegated` | 11 operations | a dedicated checker in `typing.rs` decides, as for aggregates |
| `from(field)` through `OperandTypes` | 3 operations | the ABI layout or callee computes the operand list's types |
| `AttributeType` | 2 attribute types | `type(attribute)` asks the attribute which type it denotes |
| `Operands` impls | 6 operand types | the Rust type holds operands in its own layout |
| metadata flags | 1 flag | phase legality depends on instruction metadata |
| `word_fn` bodies | 26 functions | 256-bit word arithmetic in `op/word.rs` |

The `EffectKind` meanings in `effects.rs`, the single-opcode selection rules in
`isle/mir-to-evm/select.isle`, and the lowering passes stay in Rust. They do not classify
operations: each states what a vocabulary item means, or lowers specific operations. So does
the canonical direction of a `reversible` comparison: `canonical.rs` turns `gt` into `lt` with
its operands exchanged.

Some passes still name core operations in a `match` to decide a property the schema could
state. They predate this language, treat operations they do not name conservatively, and are
the next candidates for traits:

- `lower_integers.rs` chooses how to extend narrow operands by variant (`sdiv` and the signed
  comparisons sign-extend); constant folding in `utils/eval.rs` reads it from the declared
  `semantics opcode(..)` instead;
- the e-graph's `is_node` lists the reads that are stable within a function (calldata,
  `blockhash`, `blobhash`);
- `cse.rs` (`is_path_sensitive_kind`, `is_frame_function_of_arguments`), `alias.rs`,
  `heap_floor.rs`, and `call_cleanup.rs` name operations with special memory or frame meaning;
- EVM IR passes such as `block_cse.rs` group opcodes into invalidation epochs.

## Extensions

An extension is a directory of `.ops` files whose header names a namespace:

```text
dialect mir.acme;

/// The average of two words, rounded down, without overflow.
op Avg(a: value, b: value) -> I256 {
    effect Pure;
    traits rematerializable;
    commutative a, b;
    types a: i256, b: i256;
}
```

`SOLAR_MIR_OPS_PATH` lists extension directories by absolute path, as `PATH` does; the build
script compiles them after the core files. EVM IR takes no extensions yet. The generator derives
every name of an extension operation from the namespace and the local name, so none can collide
with a later core operation or another namespace:

| Name | `op Avg` in `mir.acme` |
| --- | --- |
| `InstKind` variant | `AcmeAvg` |
| mnemonic | `acme.avg` |
| ISLE term | `acme_avg` |
| builder | `acme_avg` |

It enforces these rules on extension files:

- the header must name a namespace: a lowercase letter, then lowercase letters and digits;
- a file may declare `op`, `class`, and `trait`, and nothing else; a trait is named
  `acme_...`, and a class or base name resolves in the namespace first;
- an explicit `mnemonic` is `acme.` and one lowercase identifier;
- operations take `value` operands only, so the generic text form `acme.avg v0, v1` holds the
  whole instruction and prints and parses back unchanged;
- `effect` is required, so no extension operation becomes `Pure` by omission.

An extension operation is a full `InstKind` variant: adding one requires no Rust change for the
workspace to build. The text parser, printer, operand traversal, type verifier, phase verifier,
effects, dead-code elimination, constant folding, and e-graph value numbering all use the
declared properties. With `Avg` above, MIR text such as

```text
v0 = acme.avg arg0, arg1
v1 = acme.avg arg1, arg0
v2 = acme.avg arg0, arg0
```

parses and prints; `-Zmir-pipeline=egraph,dce` merges `v1` into `v0` through the declared
commutative pair and removes the unused `v2`, while a side-effecting extension operation stays.

What an extension cannot do yet is reach bytecode. Instruction selection and lowering are
Rust and ISLE; an operation without a selection rule makes the stack planner fail with
`instruction without a stack lowering`, never miscompile.

## Proposal

The survey behind this design is in the PR's research artifact. Its lessons, and how this
prototype applies them:

1. **One declaration per operation, with every property next to it** (Cranelift meta, Go
   `_gen`, GHC `primops.txt.pp`). LLVM's split between `TargetOpcodes.def` and `Target.td`,
   and MLIR's semantic traits implemented as C++ templates, show the cost of spreading one
   operation over several tables.
2. **No code strings in the schema.** MLIR's `CPred` and GCC's C conditions make the schema
   opaque to every tool but the C++ compiler; IRDL measured that 30% of MLIR operations need a
   C++ constraint. Here predicates and types are a small language the generator checks, and the
   remaining hooks are typed traits.
3. **Conservative defaults for what an operation does not state.** LLVM intrinsics and MLIR's
   unregistered operations assume every effect. The core schema keeps `Pure` as its default for
   brevity, and extensions must state their effect; traits are positive capabilities, so a
   missing trait never licenses a transform.
4. **Namespaced identity** (MLIR `dialect.op`, LLVM `llvm.x86.*`, SPIR-V vendor ranges, RISC-V
   custom opcode space). Extension names and mnemonics carry their namespace.
5. **New vocabulary covers every existing operation.** A new trait defaults to absent for all
   operations, the safe reading of a capability flag (the expression-problem work of Zenger and
   Odersky, and the modular well-definedness checks of Silver and ableC, make the same demand).

The next steps, in order:

1. **Selection from semantics.** Most rules in `select.isle` restate `semantics opcode(x)`.
   Generate the single-opcode rules from the declared semantics and the opcode's stack
   signature, so an extension operation with opcode semantics reaches bytecode without Rust.
2. **Expansion to core operations.** Add an `expand` property: a body of core operations that
   defines the extension operation, run by one generic lowering pass before `until` takes
   effect. Check that the declared properties agree with the expansion (an operation that
   declares `Pure` may not expand to a store) and use the expansion as the interpreter's
   semantics. This is MLIR's forwarding-to-core lesson, and it makes every extension operation
   lowerable.
3. **EVM extensions.** Let an `evm.<namespace>` file add opcodes in free byte values, with an
   availability gate that the extension also declares, so an L2 can add its opcodes and the MIR
   operations that select them.
4. **Separate compilation.** Today an extension rebuilds the compiler. A runtime
   `InstKind::Ext(ExtId, operands)` variant with static descriptor tables, generated by a third
   party's build script through `solar-op-dsl`, would let a plugin crate register operations
   without rebuilding. The textual form would round-trip unknown operations generically, as
   MLIR does, and the descriptor would carry a version for the text and bytecode formats.
