//@ codegen-matrix: standard
//@ run-call: grownHeap 5, 1, 0x80 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dee356
//@ run-call: grownHeap 5, 1, 0x2080 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dee356
//@ run-call: grownHeap 5, 0, 0x80 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec756
//@ run-call: chosenPointer 5, 1, 0x80 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: chosenPointer 5, 1, 0x2080 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: chosenPointer 5, 0, 0x80 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec756
//@ run-call: farCopy 5, 1, 0x200 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: farCopy 5, 0, 0x200 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec756
//@ run-call: callResultCopy 5, 1, 0x80 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: callResultCopy 5, 0, 0x80 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec756
//@ run-call: callResultCopy 5, 1, 0x7f => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: choiceCopy 5, 1, 0 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: choiceCopy 5, 0, 0 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec756
//@ run-call: choiceCopy 5, 1, 1 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: mergedPointer 5, 1, 32 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: mergedPointer 5, 0, 32 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec756
//@ run-call: mergedPointer 5, 1, 33 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
// The compiler keeps live spill words clear of any memory write it cannot place above the spill
// area, and keeps the free memory pointer at or above the heap floor, whatever a pass folds: each
// function holds seventeen values across the write, then allocates. A pointer an input or a call
// chooses, a phi of the pointer and a low constant, and a store through a computed address that
// reaches the pointer's slot are as able to reach a spill slot as a literal low address.
contract FoldedHazardSpills {
    function grownHeap(uint256 a, uint256 flag, uint256 d) external pure returns (bytes32 result) {
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
        _setPointer(d);
        if (flag != 0) {
            bytes memory c = _copy(0x2400);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }

    function chosenPointer(uint256 a, uint256 flag, uint256 d) external pure returns (bytes32 result) {
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
        _setPointer(d);
        if (flag != 0) {
            bytes memory c = _copy(1024);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }

    function farCopy(uint256 a, uint256 flag, uint256 n) external pure returns (bytes32 result) {
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
        _setPointer(0xf00);
        assembly {
            codecopy(0x2f00, 0, n)
            codecopy(0x2f00, 0, n)
        }
        if (flag != 0) {
            bytes memory c = _copy(1024);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }

    function callResultCopy(uint256 a, uint256 flag, uint256 d) external pure returns (bytes32 result) {
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
        uint256 p = _pick(d);
        uint256 q = _pick(d + 1);
        assembly {
            calldatacopy(p, 0, 0x40)
            calldatacopy(q, 0, 0x40)
        }
        if (flag != 0) {
            bytes memory c = _copy(1024);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }

    function choiceCopy(uint256 a, uint256 flag, uint256 d) external pure returns (bytes32 result) {
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
        assembly {
            let p := mload(0x40)
            if iszero(d) { p := 0x80 }
            calldatacopy(p, 0, 0x40)
            calldatacopy(p, 0, 0x40)
        }
        if (flag != 0) {
            bytes memory c = _copy(1024);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }

    function mergedPointer(uint256 a, uint256 flag, uint256 o) external pure returns (bytes32 result) {
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
        assembly {
            o := calldataload(0x44)
            if eq(o, 0x20) {
                let p := add(o, 0x20)
                if flag { p := mul(o, 2) }
                mstore(p, 0x100)
                mstore(p, 0x80)
            }
        }
        if (flag != 0) {
            bytes memory c = _copy(1024);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }

    function _pick(uint256 d) internal pure returns (uint256 p) {
        p = d;
        for (uint256 i = 0; i < d % 3; i++) {
            p += 0x20;
        }
        if (p < 0x80) {
            p = 0x80;
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
