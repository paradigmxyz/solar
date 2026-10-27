//@ codegen-matrix: standard
//@ run-call: copy 0x010203, 3 => 0x010203
//@ run-call: copy 0x, 0 => 0x
//@ run-call-fail: copy 0x010203, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: copy 0x01, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A fresh allocation's size does not prove its length after assembly changes its header.
contract Test {
    function copy(bytes memory source, uint256 length) external pure returns (bytes memory dest) {
        uint256 count = source.length;
        dest = new bytes(count);
        assembly {
            mstore(dest, length)
        }
        for (uint256 i; i < count; ++i) {
            dest[i] = source[i];
        }
    }
}
