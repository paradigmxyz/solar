// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

uint256 constant LIMIT = 4;

contract Yul {
    uint256 internal word;
    uint128 internal low;
    uint128 internal high;
    uint256[] internal items;

    function suffixes(bytes calldata cd, function() external callback) public returns (uint256 total) {
        uint256[] storage list = items;
        uint256 local;
        assembly ("memory-safe") {
            total := add(cd.length, cd.offset)
            total := add(total, word.slot)
            total := add(total, high.offset)
            total := add(total, callback.address)
            total := add(total, callback.selector)
            total := add(total, list.slot)
            local := LIMIT
            total := add(total, local)
        }
    }

    function functions() public returns (uint256 total) {
        assembly {
            // A function that uses declarations of the Solidity function and contract.
            function scale(x, y) -> z, w {
                z := mul(x, LIMIT)
                w := sload(word.slot)
                if iszero(y) { leave }
                {
                    w := add(w, 1)
                }
            }
            function noop() {}
            let u, v := scale(1, 2)
            let unset
            u, v := scale(u, v)
            noop()
            total := add(add(u, v), unset)
        }
    }

    function literals() public pure returns (bytes32 result) {
        assembly {
            let num := 0x20
            let flag := true
            let other := false
            let text := "ab\x01c"
            let raw := hex"c3"
            result := add(add(num, flag), add(other, text))
            result := add(result, raw)
        }
    }

    function control(uint256 n) public pure returns (uint256 total) {
        assembly {
            switch n
            case 0 { mstore(0, 1) }
            case 0x20 { total := 1 }
            default { revert(0, 0) }

            switch n
            default { }

            for { let i := 0 } lt(i, 3) { i := add(i, 1) } {
                if eq(i, 1) { continue }
                if gt(i, 2) { break }
                total := add(total, i)
            }
        }
        assembly {
            total := keccak256(0, calldatasize())
        }
    }
}
