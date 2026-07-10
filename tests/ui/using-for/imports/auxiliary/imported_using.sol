type Int is int256;

function add(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

function inc(uint256 x) pure returns (uint256) {
    return x + 1;
}

library Lib {
    function twice(uint256 x) internal pure returns (uint256) {
        return x * 2;
    }
}

using {add as +} for Int global;
