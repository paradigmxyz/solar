//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=mir

// Memory `bytes` uses the packed `[length][data...]` layout: `new bytes(n)`
// allocates 32 + pad32(n) zeroed bytes (not one word per byte), element reads
// extract single bytes left-aligned as `bytes1`, and element stores are
// single-byte `mstore8` writes at `data + i`.
contract BytesMemoryElements {
    function alloc() external pure returns (bytes32) {
        bytes memory buf = new bytes(96);
        buf[5] = 0xAA;
        buf[95] = hex"ff";
        return keccak256(buf);
    }

    function literal() external pure returns (bytes32) {
        bytes memory buf = hex"00010203040506070809";
        buf[5] = 0xAA;
        return keccak256(buf);
    }

    function allocDynamic(uint n) external pure returns (uint) {
        bytes memory buf = new bytes(n);
        return buf.length;
    }

    function readWrite(bytes memory b, uint i, bytes1 v) external pure returns (bytes1) {
        b[i] = v;
        return b[i];
    }
}
