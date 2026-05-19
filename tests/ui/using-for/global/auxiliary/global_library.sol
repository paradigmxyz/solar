// Ported from test/libsolidity/semanticTests/using/using_global_all_the_types.sol.

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
}

using L for E global;
using L for S global;
using L for T global;
