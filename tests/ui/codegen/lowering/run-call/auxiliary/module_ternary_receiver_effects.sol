import "./module_ternary_receiver_effects_nested.sol" as N;

contract D {
    function g() external pure returns (uint256) {
        return 7;
    }
}

struct S {
    uint256 a;
}

enum En { A, B, C }

type U is uint64;

uint256 constant K = 5;
string constant MSG = "short";

error E(uint256);

event Ev(uint256);

function f(uint256 x) pure returns (uint256) {
    return x + 1;
}

library L {
    uint256 constant LK = 11;

    function h(uint256 x) internal pure returns (uint256) {
        return x * 2;
    }
}
