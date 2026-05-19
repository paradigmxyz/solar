import "./global_wrong_type.sol";

function add1(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

function unsub1(Int a) pure returns (Int) {
    return Int.wrap(-Int.unwrap(a));
}

using {add1 as +} for Int global;
using {unsub1 as -} for Int global;
