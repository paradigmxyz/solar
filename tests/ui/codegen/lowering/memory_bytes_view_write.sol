//@ codegen-matrix: standard
//@ run-call-fail: w 0x6162636465

// Assembly that writes over the heap may overwrite the memory copy of a
// `bytes` argument, so the argument keeps its copy: here the clobbered
// length makes encoding run out of gas.
contract MemoryBytesViewWrite {
    function w(bytes memory b) external pure returns (bytes memory) {
        assembly {
            for { let i := 0x80 } lt(i, 0x400) { i := add(i, 1) } { mstore8(i, 0x41) }
        }
        return abi.encode(b);
    }
}
