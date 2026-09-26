# Native test linking

Foundry can select individual test-only bytecode references for the compiler to lower
directly to the existing artifact cheatcodes. This avoids generating Solidity
constructor-encoding helpers, rewriting source, and compiling those helpers.
The compiler still type-checks the original constructor call and uses its usual
typed argument lowering and ABI encoder.

This is a build-tool protocol, not a Solidity language feature or a production
linker. Ordinary compilation is unchanged. A linked artifact requires Foundry's
test runtime and must not be deployed onchain.

## Protocol version 1

Our solc-compatible version (`SOLC_WRAPPER=1`) includes `solar` and
`testlink1` in its semver build metadata. Foundry versions that understand this
marker can pass repeatable `--test-link SOURCE:END` arguments. Older Foundry
versions can continue using source preprocessing with the same compiler.

`SOURCE` is the exact source-unit name in the compilation input. `END` is the
exclusive UTF-8 byte offset at the end of `new C` (before call options or
arguments), or at the end of `type(C).creationCode`. The last colon separates
the offset so source names can contain colons. The equivalent Standard JSON
setting is `settings.solarTestLinks`, an array of `"SOURCE:END"` strings.
Nonempty selections are recorded in metadata for the affected source closure.

The build tool owns selection and cache invalidation. In particular, it must
retain native dependencies for production contracts, scripts, mocks,
bytecode-sensitive code, constant expressions, and `try new` expressions.
Selection must use the exact sources sent to the compiler. We reject an
unmatched location or a location in a constant or `try new` expression rather than
silently retaining an embedded dependency that the cache assumes is dynamic.

Selected references are lowered as follows:

| Reference | Generated operation |
| --- | --- |
| `new C()` | `vm.deployCode("source:C")` |
| `new C(args)` | `vm.deployCode("source:C", abi.encode(args))` |
| `new C{value: v}(args)` | `vm.deployCode("source:C", abi.encode(args), v)` |
| `new C{salt: s, value: v}(args)` | `vm.deployCode("source:C", abi.encode(args), v, s)` |
| `type(C).creationCode` | `vm.getCode("source:C")` |

Absent constructor arguments and value options use the corresponding shorter
cheatcode overload rather than allocating an empty argument buffer.

We do not add a static bytecode dependency for a selected reference. Other
references to the same contract remain native, and explicitly requested child
artifacts are still compiled. Targets before Byzantium are unsupported.

This retains the existing dynamic-linking runtime tradeoffs, including the
artifact lookup and cheatcode call. It is not transparent for gas-sensitive
tests or code that observes return buffers or call depth. Test the production
build with dynamic linking disabled when those differences matter.

`forge-std` remains ordinary versioned Solidity source. Its local modifications
and pinned revision continue to participate in type checking and dependency
invalidation; this protocol does not substitute a hard-coded standard library.

The cross-repository regression fixture lives under
`crates/codegen/testdata/foundry-test-linking`. With a Foundry build supporting
this protocol on `PATH`, run
`cargo nextest run -p solar-tester native_test_linking_cache --run-ignored=only`.
It checks deployment behavior, native fallback, and artifact reuse after a
constructor-body edit. It is separate from the default Foundry fixture suite,
which also supports older Foundry versions.
