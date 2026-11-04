//@ revisions: gas size
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefix=GAS
//@[size] filecheck: --check-prefix=SIZE
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: one 3, 7 => 5
//@ run-call: two 3, 7 => 6
//@ run-call: three 3, 7 => 4
//@ run-call-fail: one 3, 0 => Panic(0x12)

// The precompile call's success flag and result reach `inverse`'s join through phis, and the
// join branches on the flag. The join takes the flag on top, the order its branch consumes the
// words in, so the call's block pays the shuffle and the join starts with `iszero` instead of
// a `swap 1` first.
// GAS: staticcall
// GAS: jump [[JOIN:bb[0-9]+]]
// GAS-NEXT: [[JOIN]]:
// GAS-NEXT: iszero
//
// At `-O size` the join drops two dead words first.
// SIZE: staticcall
// SIZE: jump [[JOIN:bb[0-9]+]]
// SIZE-NEXT: [[JOIN]]:
// SIZE-NEXT: pop
// SIZE-NEXT: pop
// SIZE-NEXT: swap 2
// SIZE-NEXT: iszero
contract JoinWantedOrder {
    function tryModExp(uint256 b, uint256 e, uint256 m) internal view returns (bool success, uint256 result) {
        if (m == 0) return (false, 0);
        assembly ("memory-safe") {
            let ptr := mload(0x40)
            mstore(ptr, 0x20)
            mstore(add(ptr, 0x20), 0x20)
            mstore(add(ptr, 0x40), 0x20)
            mstore(add(ptr, 0x60), b)
            mstore(add(ptr, 0x80), e)
            mstore(add(ptr, 0xa0), m)
            success := staticcall(gas(), 0x05, ptr, 0xc0, 0x00, 0x20)
            result := mload(0x00)
        }
    }

    function modExp(uint256 b, uint256 e, uint256 m) internal view returns (uint256 result) {
        bool success;
        (success, result) = tryModExp(b, e, m);
        if (!success) {
            assembly {
                mstore(0, 0x4e487b71)
                mstore(0x20, 0x12)
                revert(0x1c, 0x24)
            }
        }
    }

    function inverse(uint256 a, uint256 p) internal view returns (uint256) {
        unchecked {
            return modExp(a, p - 2, p);
        }
    }

    function one(uint256 a, uint256 p) external view returns (uint256) {
        return inverse(a, p);
    }

    function two(uint256 a, uint256 p) external view returns (uint256) {
        return inverse(a, p) + inverse(p, a);
    }

    function three(uint256 a, uint256 p) external view returns (uint256) {
        return inverse(a + 1, p) * inverse(p + 1, a) * inverse(p, a + 3);
    }
}
