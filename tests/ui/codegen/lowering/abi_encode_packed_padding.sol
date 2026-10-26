//@ codegen-matrix: standard
//@ run-call: padding 0x346d4ccbe8d52269524e442c43b00b08584f8bd9 => 0
//@ run-call: fromCalldata 0x5af43d3d93803e602a57fd5bf3 => 0
//@ run-call: wordAligned 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => 0

// The bytes after a packed encoding's end in its last word read as zero, as
// they do with solc, even when the memory the result takes was dirty: code that
// reads the result a word at a time, such as Solady's memory checks, relies on
// it. Each case first fills the free memory with ones.

contract EncodePackedPadding {
    bytes private constant HEAD = hex"602c3d8160093d39f33d3d3d3d363d3d37363d73";
    bytes private constant TAIL = hex"5af43d3d93803e602a57fd5bf3";

    function padding(address implementation) external pure returns (uint256) {
        _dirty();
        return _tail(abi.encodePacked(HEAD, implementation, TAIL));
    }

    function fromCalldata(bytes calldata data) external pure returns (uint256) {
        _dirty();
        return _tail(abi.encodePacked(uint8(1), data));
    }

    function wordAligned(bytes calldata data) external pure returns (uint256) {
        _dirty();
        return _tail(abi.encodePacked(data, uint8(7)));
    }

    function _dirty() private pure {
        assembly ("memory-safe") {
            let m := mload(0x40)
            for { let i := 0 } lt(i, 0x400) { i := add(i, 0x20) } { mstore(add(m, i), not(0)) }
        }
    }

    function _tail(bytes memory b) private pure returns (uint256 tail) {
        assembly ("memory-safe") {
            let length := mload(b)
            let last := mload(add(add(b, 0x20), and(length, not(0x1f))))
            tail := shl(mul(8, and(length, 0x1f)), last)
        }
    }
}
