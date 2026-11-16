//@ codegen-matrix: standard
//@ run-call: highBuffer 5, 4 => 5, 5, 24

// A recursive helper places every frame above the highest end of all routes. One route lays memory
// out at an address calldata sizes, which raises its end above low memory, so the frames another
// route reaches must also clear the fixed buffer that route keeps there.
contract AssemblyLowMemoryRecursiveRoutes {
    function layout(uint256 a, uint256 words) external pure returns (bytes32 h) {
        assembly {
            let end := add(0xa0, shl(5, calldataload(0x24)))
            mstore(end, a)
            h := keccak256(0xa0, add(sub(end, 0xa0), 0x20))
        }
        words;
    }

    function highBuffer(uint256 a, uint256 n)
        external
        pure
        returns (uint256 w0, uint256 w1, uint256 r)
    {
        assembly {
            function f(x) -> y {
                switch x
                case 0 { y := 1 }
                default { y := mul(x, f(sub(x, 1))) }
            }
            for { let p := 0x2080 } lt(p, 0x2880) { p := add(p, 0x20) } { mstore(p, a) }
            r := f(n)
            w0 := mload(0x2080)
            w1 := mload(0x20a0)
        }
    }
}
