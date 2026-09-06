//@ codegen-matrix: standard
//@ run-call: run 0, 0, 0, 0 => 18
//@ run-call: run 1, 2, 3, 4 => 38
//@ run-call: run 3, 5, 7, 11 => 102
//@ run-call: run 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffda

// Nested calls materialize absent arguments while keeping the caller's live
// value and return label intact. The best load order may differ from ABI order.
contract CallMaterializationOrder {
    function run(uint256 a, uint256 b, uint256 c, uint256 retained)
        external pure returns (uint256)
    {
        return outer(a, b, c, retained);
    }

    function outer(uint256 a, uint256 b, uint256 c, uint256 retained)
        internal pure returns (uint256 total)
    {
        unchecked {
            for (uint256 i; i != 2; ++i) total += fold(a, b, c) + retained;
        }
    }

    function fold(uint256 a, uint256 b, uint256 c) internal pure returns (uint256 total) {
        unchecked {
            for (uint256 i; i != 3; ++i) total += (a ^ i) + (b ^ i) + (c ^ i);
        }
    }
}
