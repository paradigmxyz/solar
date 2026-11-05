# Frontend plugins

A Rust library can register expression extensions, statement extensions and typed
builtins alongside its MIR and EVM IR passes. Load it explicitly with
`-Zload-pass-plugin`. Libraries are loaded before parsing and retained for the
compilation. Their names, versions and binary hashes identify the build inputs.

Build the example with the compiler's SDK and Rust toolchain:

```sh
cargo build -p solar-compiler -p solar-pass-plugin-example
target/debug/solar -Zload-pass-plugin=target/debug/libsolar_pass_plugin_example.so \
    --emit=bin-runtime example.sol
```

On macOS, use `.dylib` instead of `.so`. See the
[SDK guide](../crates/pass-plugin/README.md) for build compatibility and exports.
A frontend-only library may register an empty `passes` list.

## Expression expansion

```solidity
contract Example {
    function answer() external pure returns (uint256) {
        return @example.sum(20, @example.sum(21, 1));
    }
}
```

The library registers `sum` as an `ExtensionKind::Expression` and returns:

```rust
Expansion::Expression(ExpansionExpr::Binary {
    op: "+".into(),
    left: Box::new(ExpansionExpr::Argument(0)),
    right: Box::new(ExpansionExpr::Argument(1)),
})
```

The host builds an ordinary addition AST. Name resolution, type checking, purity
checking and checked arithmetic follow the normal compiler path. Overflow panics
as it does for a handwritten addition. Generated operations use the invocation's
source span; argument diagnostics retain the argument's location.

Recipes can use word literals, arithmetic, shifts and bitwise operators. Each
argument must appear exactly once and in source order. Recipes that duplicate,
omit or reorder arguments are rejected. Syntax expansions nest and can contain
typed builtin calls.

## Statement expansion

```solidity
contract Example {
    function total(uint256 count) external pure returns (uint256 result) {
        @example.repeat(count) {
            result += 7;
        }
    }
}
```

Register `repeat` as an `ExtensionKind::Statement` and return:

```rust
Expansion::Statement(ExpansionStmt::Repeat {
    count: ExpansionExpr::Argument(0),
    body: Box::new(ExpansionStmt::Body),
})
```

The parser accepts a parenthesized argument list followed by a block. It binds
all arguments once, in source order, to generated `uint256` locals, then builds
ordinary blocks and loops from the recipe. This also holds when a recipe nests
several loops. Generated identifiers contain characters source identifiers
cannot contain, so they cannot shadow user variables.

`Repeat` evaluates its count once and runs the body that many times. `break` and
`continue` target the generated loop; `return` exits the function. A zero count
evaluates the argument but skips the body. Solidity checks still apply inside
the body, including arithmetic checks and function mutability.

`Block` groups statement recipes. A recipe must use its input arguments and its
body once; it cannot duplicate or discard the body AST. Both expression and
statement expansion nesting are limited to 128 levels.

Expansion callbacks receive the invocation's source tokens, including their
file-relative byte ranges. The compiler parses arguments and the body; plugins
return structured recipes, rather than source strings to paste into the file.
The extension point does not change operator precedence or declaration syntax.

## Typed builtins and MIR

```solidity
contract Example {
    function rotate(uint256 value, uint256 shift)
        external pure returns (uint256)
    {
        return @example.rotl(value, shift);
    }
}
```

Register the signature in `FrontendManifest.builtins`:

```rust
BuiltinDefinition {
    name: "rotl".into(),
    signature: BuiltinSignature {
        parameters: vec![BuiltinType::Uint256; 2],
        result: BuiltinType::Uint256,
        effect: BuiltinEffect::Pure,
    },
}
```

The frontend checks argument count and implicit conversion to the parameter
types. During HIR-to-MIR lowering, `Plugin::lower_builtin("rotl")` returns a
`BuiltinBody`. Its instructions name existing MIR operations; operands name
arguments, previous instruction results, or word constants. For example:

```rust
BuiltinBody {
    instructions: vec![BuiltinInstruction {
        operation: "xor".into(),
        operands: vec![BuiltinOperand::Argument(0), BuiltinOperand::Argument(1)],
    }],
    result: BuiltinOperand::Local(0),
}
```

This fragment implements XOR. The example library's `rotl` recipe uses `and`,
`shl`, `sub`, `shr` and `or`; shifts are reduced modulo 256. Operand order is MIR
order: `shl` takes the shift count before the value. These operations have EVM
word semantics, including wrapping arithmetic; they do not introduce Solidity
arithmetic checks.

The first builtin API supports pure `uint256` arguments and one `uint256` result.
`Unknown`, the default effect, is rejected. Fragments cannot access memory,
storage, the environment or calls, or introduce control flow or pointer values.
They contain at most 4,096 instructions and can reference only earlier results.
The host checks operation contracts and verifies an isolated MIR function before
emitting the fragment, even with `-Zvalidate-ir=false`. Arguments are evaluated
once before the emitted fragment uses them. Normal optimization, stack scheduling
and EVM assembly handle the result. Builtins are runtime expressions; they cannot
initialize Solidity constants.

Native plugins are trusted executable code. The SDK checks build compatibility
and contains callback panics. Structural verification does not prove that a
plugin implements its advertised semantics. Custom backend artifacts and new VM
opcodes require a separate interface and are outside this frontend extension.
