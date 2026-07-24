//@ compile-flags: -Zcodegen --emit=bin-runtime

contract FallbackBytesUnsupported { //~ ERROR: codegen does not support `fallback(bytes) returns (bytes)` yet
    fallback(bytes calldata input) external returns (bytes memory) {
        return input;
    }
}
