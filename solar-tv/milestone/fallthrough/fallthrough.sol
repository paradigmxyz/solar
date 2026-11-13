contract Fallthrough {
    uint256 x;
    function f(uint256 a, uint256 b) external {
        if (a > b) { x = a; } else { x = b; }
    }
    function g(uint256 a) external {
        if (a == 0) { x = 1; } else { x = a; }
    }
    function h(uint256 a, uint256 b) external {
        if (a != b) { x = 2; } else { x = 3; }
    }
}
