//@ codegen-matrix: standard
//@ run-call: backCounter 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: backCounter 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: backLoop 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: backLoop 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: laundered 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: laundered 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: forgedByCopy 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: forgedByCopy 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe738
//@ run-call: forgedByByteStore 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: forgedByByteStore 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe738
//@ run-call: forgedByLoop 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: forgedByLoop 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe738
//@ run-call: stepFromFloor 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: stepFromFloor 5, 0x80 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: lowerPointer 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: lowerPointer 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe738
//@ run-call: recursiveStepBack 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: recursiveStepBack 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: retaggedResult 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: retaggedResult 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe738
//@ run-call: misalignedStore 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: misalignedStore 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe738
// Writes whose destination the compiler cannot place above the spills, through a pointer that
// steps back from the heap by an offset it cannot bound, by a loop, or through a word that memory
// launders, keep the values live across them out of the spill area. Words that a copy, a byte
// store, or a store through a computed address leaves in the free memory pointer's slot are
// raised before an allocation reads them. Each function holds twenty-four values across the
// write.
contract SpillWriteProvenance {
    function backCounter(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        if (flag != 0) {
            assembly {
                let p := mload(0x40)
                let n := div(sub(p, 0x80), 0x20)
                for { let i := 1 } iszero(gt(i, n)) { i := add(i, 1) } {
                    mstore(add(p, mul(i, not(31))), 0)
                }
            }
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }

    function backLoop(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        if (flag != 0) {
            assembly {
                let p := mload(0x40)
                for { let q := p } gt(q, 0x80) { q := sub(q, 0x20) } {
                    mstore(sub(q, 0x20), 0)
                }
            }
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }

    function laundered(uint256 a, uint256 flag) external view returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        if (flag != 0) {
            assembly {
                let p := mload(0x40)
                pop(staticcall(gas(), 4, 0x40, 0x20, 0, 0x20))
                let q := mload(0)
                calldatacopy(add(p, sub(0x80, q)), calldatasize(), sub(q, 0x80))
            }
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }

    function forgedByCopy(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        if (flag != 0) {
            assembly {
                mstore(0x20, 0x80)
                calldatacopy(0x40, 0x44, 0x20)
            }
        }
        if (flag != 0) {
            bytes memory c = _copy(0x400);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }

    function forgedByByteStore(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        if (flag != 0) {
            assembly {
                mstore(0x50, shl(128, 0x80))
            }
        }
        if (flag != 0) {
            bytes memory c = _copy(0x400);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }

    function forgedByLoop(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        if (flag != 0) {
            assembly {
                let p := mload(0x40)
                for { let i := 1 } lt(i, div(p, 0x20)) { i := add(i, 1) } {
                    let addr := add(p, mul(i, not(31)))
                    if eq(addr, 0x40) { mstore(addr, 0x80) }
                }
            }
        }
        if (flag != 0) {
            bytes memory c = _copy(0x400);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }

    function stepFromFloor(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        if (flag != 0) {
            assembly {
                mstore(0x40, flag)
                let p := mload(0x40)
                mstore(sub(p, 0x20), 0)
            }
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }

    function lowerPointer(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        if (flag != 0) {
            assembly {
                mstore(0x40, sub(mload(0x40), 0x20))
            }
        }
        if (flag != 0) {
            bytes memory c = _copy(0x400);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }

    function recursiveStepBack(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        if (flag != 0) {
            uint256 p;
            assembly {
                p := mload(0x40)
            }
            _stepBack(p, 12);
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }

    function retaggedResult(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        if (flag != 0) {
            bytes memory b = _retag(0x80, 3);
            assembly {
                mstore(0x40, b)
            }
        }
        if (flag != 0) {
            bytes memory c = _copy(0x400);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }

    function misalignedStore(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        if (flag != 0) {
            assembly {
                mstore(0x50, mload(0x40))
            }
        }
        if (flag != 0) {
            bytes memory c = _copy(0x400);
            result = bytes32(c.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }

    function _stepBack(uint256 p, uint256 n) internal pure {
        if (n == 0) return;
        assembly {
            mstore(sub(p, 0x20), 0)
        }
        _stepBack(p - 0x20, n - 1);
    }

    function _retag(uint256 x, uint256 n) internal pure returns (bytes memory b) {
        if (n != 0) return _retag(x, n - 1);
        assembly {
            b := x
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
