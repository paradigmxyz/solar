# Frontend plugins

Frontend plugins use the same Rust SDK and `-Zload-pass-plugin` option as
[pass plugins](../crates/pass-plugin/README.md). Rebuild the plugin with the
compiler's SDK and Rust toolchain.

## Expression example

Build the example library:

```sh
cargo build -p solar-pass-plugin-example
```

On Linux, compile this file with
`solar -Zload-pass-plugin=target/debug/libsolar_pass_plugin_example.so example.sol`.
On macOS, use the `.dylib` suffix.

```solidity
contract Example {
    function answer() external pure returns (uint256) {
        return @example.sum(20, @example.sum(21, 1));
    }
}
```

The plugin registers `sum` as an expression extension and returns this recipe:

```rust
Expansion::Expression(ExpansionExpr::Binary {
    op: "+".into(),
    left: Box::new(ExpansionExpr::Argument(0)),
    right: Box::new(ExpansionExpr::Argument(1)),
})
```

The parser constructs an ordinary addition AST. Name resolution, type checking,
purity checking and checked arithmetic then follow the normal compiler path.
Overflow panics as it does for a handwritten addition. Nested invocations work.
Generated operations use the invocation's source span for diagnostics.

The current expression adapter requires every argument exactly once, in order.
It rejects recipes that duplicate, omit or reorder arguments. It accepts
arithmetic, shifts and bitwise binary operators. Literal recipes, statement
expansions and typed builtins are not yet connected to the compiler driver.
The SDK's repeat callback is a callback example, not executable syntax yet.

Native plugins are trusted executable code. The SDK checks build compatibility
and contains callback panics; it does not sandbox a library or prove that its
expansion has the semantics its author intended.
