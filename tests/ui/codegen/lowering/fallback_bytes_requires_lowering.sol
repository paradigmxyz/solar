//@ compile-flags: -Zcodegen --emit=bin-runtime

contract FallbackBytesRequiresLowering { //~ ERROR: EVM codegen requires MIR in the `evm-shaped` phase, stopped at `abi`
    fallback(bytes calldata input) external returns (bytes memory) {
        return input;
    }
}
