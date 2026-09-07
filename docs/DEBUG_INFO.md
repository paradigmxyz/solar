# Debug outputs

Request debug artifacts through `--emit`, alongside ABI or bytecode outputs:

```sh
mkdir -p out
solar "$PWD/Counter.sol" --out-dir out \
  --emit=abi,bin,bin-runtime,ethdebug,ethdebug-runtime,srcmap,srcmap-runtime
```

The result is `out/combined.json`. Without `--out-dir`, JSON goes to stdout;
`--pretty-json` enables indentation. Absolute input paths, as above, let
source-map consumers find source files independently of the output directory.

| Output | JSON field | Contents |
| --- | --- | --- |
| `ethdebug` | `contracts["path:Contract"].ethdebug` | Creation program |
| `ethdebug-runtime` | `contracts["path:Contract"]["ethdebug-runtime"]` | Runtime program |
| `ethdebug-resources` | `ethdebug` | Compilation and source resources only |
| `srcmap` | `contracts["path:Contract"].srcmap` | Legacy creation source map |
| `srcmap-runtime` | `contracts["path:Contract"]["srcmap-runtime"]` | Legacy runtime source map |

Either ETHDebug program output automatically includes the top-level `ethdebug`
resource bundle, including source contents. `ethdebug-resources` alone does not
run code generation. Either legacy map output includes `sourceList`, whose
indices are the maps' source IDs. Bytecode is emitted only when requested with
`bin` or `bin-runtime`.

Both formats use the same serializers as Standard JSON's
`evm.bytecode.ethdebug`, `evm.deployedBytecode.ethdebug`,
`evm.bytecode.sourceMap`, and `evm.deployedBytecode.sourceMap` selections.
Standard JSON input and output remain unchanged. There is no separate debug
compilation mode: requesting metadata must not change executable bytecode.
Locations that optimization cannot preserve accurately remain unknown.

## SolDB artifacts

SolDB can read legacy maps directly from `combined.json`. For ETHDebug, extract
the resource bundle and the desired contract's programs into its artifact
directory:

```sh
jq '.ethdebug' out/combined.json > out/ethdebug_resources.json
jq --arg c "$PWD/Counter.sol:Counter" '.contracts[$c].ethdebug' \
  out/combined.json > out/Counter_ethdebug.json
jq --arg c "$PWD/Counter.sol:Counter" '.contracts[$c]["ethdebug-runtime"]' \
  out/combined.json > out/Counter_ethdebug-runtime.json
```

Use that directory with SolDB's `--ethdebug-dir ADDRESS:Counter:out` option for
debugging, profiling, or debug-info differential tests. No Standard JSON input
request or source-map format conversion is needed.
