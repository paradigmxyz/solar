//@ codegen-matrix: standard
//@ run-call: f 0x0102 => 3

// Inline assembly writes the copy through an address that does not derive from
// it, so the encoding must read the copy rather than the calldata slice.
contract EncodeCalldataCopyObserved {
    function f(bytes calldata data) external pure returns (uint8) {
        uint256 p;
        assembly { p := mload(0x40) }
        bytes memory copy = data;
        assembly { mstore8(add(p, 32), 3) }
        bytes memory e = abi.encode(copy);
        return uint8(e[64]);
    }
}
