//@ compile-flags: -Ogas
//@ run-call: first 4 => 58
//@ run-call: first 9 => 100
//@ run-call: second 4 => 82

// Forwarding another six-result internal call as a function's own return
// leaves adopted copies of every returned word on the stack; the return
// shuffle cannot always drop the surplus and must fall back to frame-backed
// returns instead of panicking (Seaport's `getFulfillments` shape).
contract MultiReturnForward {
    function inner(uint256 a)
        internal
        pure
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        unchecked {
            return (a + 1, a * 3, a ^ 5, a + 7, a * 11, a + 13);
        }
    }

    function outer(uint256 a)
        internal
        pure
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        return inner(a);
    }

    function first(uint256 a) external pure returns (uint256) {
        (uint256 x1, , uint256 x3, , , uint256 x6) = outer(a);
        unchecked {
            return x1 + x3 * 2 + x6 * 3;
        }
    }

    function second(uint256 a) external pure returns (uint256) {
        (, uint256 x2, , uint256 x4, uint256 x5, ) = outer(a + 1);
        unchecked {
            return x2 + x4 + x5;
        }
    }
}
