//@ codegen-matrix: standard
//@ run-call: tailedTwice 5, 1 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: tailed 5, 1 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: tailed 5, 0 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec756
//@ run-call: other 5, 1 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
// A constant stored into the free memory pointer reaches an allocation that a tail call makes,
// directly or through another tail call. The shared `_finish` makes the spill area depend on the
// entries that reach it, so this stays apart from the cases that raise it by other means.
contract FoldedTailCallSpills {
    function tailed(uint256 a, uint256 flag) external pure returns (bytes32) {
        _setPointer(0x80);
        _finish(a, flag);
    }

    function other(uint256 a, uint256 flag) external pure returns (bytes32) {
        _finish(a, flag);
    }

    function tailedTwice(uint256 a, uint256 flag) external pure returns (bytes32) {
        _setPointer(0x80);
        _middle(a, flag);
    }

    function _middle(uint256 a, uint256 flag) internal pure {
        if (flag > 1) revert();
        _finish(a, flag);
    }

    function _finish(uint256 a, uint256 flag) internal pure {
        bytes32 result;
        bytes32 h0 = keccak256(abi.encodePacked(a, uint256(0)));
        bytes32 h1 = keccak256(abi.encodePacked(a, uint256(1)));
        bytes32 h2 = keccak256(abi.encodePacked(a, uint256(2)));
        bytes32 h3 = keccak256(abi.encodePacked(a, uint256(3)));
        bytes32 h4 = keccak256(abi.encodePacked(a, uint256(4)));
        bytes32 h5 = keccak256(abi.encodePacked(a, uint256(5)));
        bytes32 h6 = keccak256(abi.encodePacked(a, uint256(6)));
        bytes32 h7 = keccak256(abi.encodePacked(a, uint256(7)));
        bytes32 h8 = keccak256(abi.encodePacked(a, uint256(8)));
        bytes32 h9 = keccak256(abi.encodePacked(a, uint256(9)));
        bytes32 h10 = keccak256(abi.encodePacked(a, uint256(10)));
        bytes32 h11 = keccak256(abi.encodePacked(a, uint256(11)));
        bytes32 h12 = keccak256(abi.encodePacked(a, uint256(12)));
        bytes32 h13 = keccak256(abi.encodePacked(a, uint256(13)));
        bytes32 h14 = keccak256(abi.encodePacked(a, uint256(14)));
        bytes32 h15 = keccak256(abi.encodePacked(a, uint256(15)));
        bytes32 h16 = keccak256(abi.encodePacked(a, uint256(16)));
        if (flag != 0) {
            bytes memory c = _copy(1024);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
        assembly {
            mstore(0, result)
            return(0, 0x20)
        }
    }

    function _setPointer(uint256 p) internal pure {
        assembly {
            mstore(0x40, p)
        }
    }

    function _copy(uint256 n) internal pure returns (bytes memory out) {
        assembly {
            out := mload(0x40)
            mstore(out, n)
            codecopy(add(out, 0x20), 0, n)
            mstore(0x40, add(add(out, 0x20), n))
        }
    }
}
