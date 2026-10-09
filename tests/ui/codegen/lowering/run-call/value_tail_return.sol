//@ codegen-matrix: standard
//@ run-call: run 0 => 12
//@ run-call: run 2 => 17
//@ run-call: run 5 => 47

contract ValueTailReturn {
    uint256 private state;

    function run(uint256 n) external returns (uint256) {
        return forward(n) + forward(n + 1);
    }

    function forward(uint256 n) internal returns (uint256) {
        unchecked {
            for (uint256 i; i < n; ++i) state += i;
            return leaf();
        }
    }

    function leaf() internal view returns (uint256 seed) {
        unchecked {
            seed = state;
            for (uint256 i; i < 4; ++i) seed += i;
            return seed;
        }
    }
}
