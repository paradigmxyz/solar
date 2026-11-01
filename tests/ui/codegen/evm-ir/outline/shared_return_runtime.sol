//@ codegen-matrix: standard
//@ run-call: run 0 => 1, 192
//@ run-call: run 5 => 6, 192
//@ run-call: run 6 => 7, 64
//@ run-call: run 12 => 13, 64
//@ run-call-fail: run 13

// Seven short bumps and six larger bumps share a return suffix when outlined for size.
// Check both entries and their continuations preserve the returned pointer and FMP bump.
contract SharedReturn {
    function run(uint256 which) external pure returns (uint256, uint256) {
        assembly {
            let p := 0
            switch which
            case 0 { p := mload(0x40) mstore(0x40, add(p, 192)) mstore(p, 1) }
            case 1 { p := mload(0x40) mstore(0x40, add(p, 192)) mstore(p, 2) }
            case 2 { p := mload(0x40) mstore(0x40, add(p, 192)) mstore(p, 3) }
            case 3 { p := mload(0x40) mstore(0x40, add(p, 192)) mstore(p, 4) }
            case 4 { p := mload(0x40) mstore(0x40, add(p, 192)) mstore(p, 5) }
            case 5 { p := mload(0x40) mstore(0x40, add(p, 192)) mstore(p, 6) }
            case 6 { p := mload(0x40) mstore(0x40, add(p, 64)) mstore(p, 7) }
            case 7 { p := mload(0x40) mstore(0x40, add(p, 64)) mstore(p, 8) }
            case 8 { p := mload(0x40) mstore(0x40, add(p, 64)) mstore(p, 9) }
            case 9 { p := mload(0x40) mstore(0x40, add(p, 64)) mstore(p, 10) }
            case 10 { p := mload(0x40) mstore(0x40, add(p, 64)) mstore(p, 11) }
            case 11 { p := mload(0x40) mstore(0x40, add(p, 64)) mstore(p, 12) }
            case 12 { p := mload(0x40) mstore(0x40, add(p, 64)) mstore(p, 13) }
            default { revert(0, 0) }
            mstore(0, mload(p))
            mstore(32, sub(mload(0x40), p))
            return(0, 64)
        }
    }
}
