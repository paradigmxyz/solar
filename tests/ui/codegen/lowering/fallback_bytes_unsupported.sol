//@ codegen-matrix: standard

contract FallbackBytesUnsupported { //~[none,gas,size] ERROR: codegen does not support `fallback(bytes) returns (bytes)` yet
    fallback(bytes calldata input) external returns (bytes memory) {
        return input;
    }
}
