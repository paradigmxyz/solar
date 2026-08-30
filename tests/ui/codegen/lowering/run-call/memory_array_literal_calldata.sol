//@ codegen-matrix: standard
//@ run-call: collect(bytes,bytes) 0x, 0x11 => [0x, 0x11]

contract MemoryArrayLiteralCalldata {
    function collect(bytes calldata first, bytes calldata second)
        external
        pure
        returns (bytes[2] memory)
    {
        return [first, second];
    }
}
