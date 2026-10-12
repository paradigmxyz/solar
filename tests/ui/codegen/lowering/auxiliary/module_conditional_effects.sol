contract B {
    address owner;
    function() internal returns (uint256) fp;
}

contract D {
    function g() external pure returns (uint256) {
        return 7;
    }
}

struct S {
    uint256 a;
}

uint256 constant K = 5;

library L {
    function ext(uint256 x) external pure returns (uint256) {
        return x + 1;
    }
}
