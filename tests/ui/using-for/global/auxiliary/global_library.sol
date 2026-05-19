enum E {
    A,
    B
}

struct S {
    uint256 x;
}

type T is uint256;

library L {
    function f(E e) internal pure returns (uint256) {
        return uint256(e);
    }

    function f(S memory s) internal pure returns (uint256) {
        return s.x;
    }

    function f(T t) internal pure returns (uint256) {
        return T.unwrap(t);
    }

    function inc(T t) internal pure returns (T) {
        return T.wrap(T.unwrap(t) + 1);
    }

    function dec(T t) external pure returns (T) {
        return T.wrap(T.unwrap(t) - 1);
    }
}

contract Maker {
    function make() external pure returns (T) {
        return T.wrap(1);
    }
}

using L for E global;
using L for S global;
using L for T global;
using {unwrap} for T global;

function unwrap(T t) pure returns (uint256) {
    return T.unwrap(t);
}
