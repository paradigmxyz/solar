//@ codegen-matrix: standard
//@ run-call: choose 3, 5, 7, 0 => 3, 5, 7
//@ run-call: choose 3, 5, 7, 1 => 3, 3, 7
//@ run-call: choose 2, 5, 7, 1 => 3, 5, 7
//@ run-call: choose 2, 5, 7, 2 => 3, 3, 7
//@ run-call: choose 0, 0, 0, 64 => 1, 1, 0
//@ run-call: choose 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 5, 7, 3 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 7

// One branch makes both loop-carried words equal. The edge shuffle must duplicate
// the resident word instead of treating its second occurrence as a deferred constant.
contract DuplicateEdgeWords {
    function choose(uint256 a, uint256 b, uint256 c, uint256 n)
        public pure returns (uint256, uint256, uint256)
    {
        unchecked {
            while (n != 0) {
                if (a & 1 != 0) {
                    b = a;
                } else {
                    a += 1;
                }
                --n;
            }
        }
        return (a, b, c);
    }
}
