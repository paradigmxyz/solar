// ported-from: test/libsolidity/semanticTests/using/using_global_invisible.sol

type T is uint256;

library L {
    function inc(T t) internal pure returns (T) {
        return T.wrap(T.unwrap(t) + 1);
    }

    function dec(T t) external pure returns (T) {
        return T.wrap(T.unwrap(t) - 1);
    }
}

using L for T global;
using {unwrap} for T global;

function unwrap(T t) pure returns (uint256) {
    return T.unwrap(t);
}
