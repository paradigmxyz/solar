//@ codegen-matrix: standard
//@ run-call: first (17, 29) => 17
//@ run-call: second (17, 29) => 29
//@ run-call: choose (17, 29), true => 17
//@ run-call: choose (17, 29), false => 29
//@ run-call: first (0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0) => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: memoryFirst (17, 29) => 17

contract SliceFields {
    struct Pair { uint256 first; uint256 second; }

    function first(Pair calldata pair) external pure returns (uint256) {
        return pair.first;
    }

    function second(Pair calldata pair) external pure returns (uint256) {
        return pair.second;
    }

    function choose(Pair calldata pair, bool chooseFirst) external pure returns (uint256) {
        if (chooseFirst) return pair.first;
        return pair.second;
    }

    function memoryFirst(Pair memory pair) external pure returns (uint256) {
        return pair.first;
    }
}
