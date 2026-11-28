//@ codegen-matrix: standard
//@ run-call: index() => 0x2986c0e500000000000000000000000000000000000000000000000000000000
//@ run-call: slice() => 0x7f37162400000000000000000000000000000000000000000000000000000000

// The element offset of a calldata index or slice start wraps modulo `2**256`, as in solc, and does
// not panic. Only assembly can forge a length large enough to overflow it.
contract C {
    function index() external pure returns (uint256) {
        uint256[] calldata a;
        assembly {
            a.offset := 0
            a.length := shl(255, 1)
        }
        return a[1 << 251];
    }

    function slice() external pure returns (uint256) {
        uint256[] calldata a;
        assembly {
            a.offset := 0
            a.length := shl(255, 1)
        }
        uint256 start = 1 << 251;
        return a[start:start + 1][0];
    }
}
