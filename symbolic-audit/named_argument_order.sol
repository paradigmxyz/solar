// Named arguments written out of declaration order. solc evaluates them in
// the order written in the source; solar evaluates them in declaration order.
// `tick(n)` appends `n` to a decimal log so the order is observable.
contract NamedArgumentOrder {
    uint256 log_;

    struct S {
        uint256 x;
        uint256 y;
    }

    S s;

    function tick(uint256 tag) internal returns (uint256) {
        log_ = log_ * 10 + tag;
        return tag;
    }

    function take(uint256 a, uint256 b) internal pure returns (uint256) {
        return a * 10 + b;
    }

    // solc 1221 (log 12, x 2, y 1); solar 2121 (log 21, x 2, y 1).
    function structNamed() external returns (uint256) {
        s = S({y: tick(1), x: tick(2)});
        return log_ * 100 + s.x * 10 + s.y;
    }

    function structPositional() external returns (uint256) {
        s = S(tick(1), tick(2));
        return log_ * 100 + s.x * 10 + s.y;
    }

    function callNamed() external returns (uint256) {
        uint256 r = take({b: tick(1), a: tick(2)});
        return log_ * 100 + r;
    }

    function callPositional() external returns (uint256) {
        uint256 r = take(tick(1), tick(2));
        return log_ * 100 + r;
    }

    function externalNamed() external returns (uint256) {
        this.recv({b: tick(1), a: tick(2)});
        return log_;
    }

    function recv(uint256 a, uint256 b) external pure {
        a;
        b;
    }

    function memoryStructNamed() external returns (uint256) {
        S memory m = S({y: tick(1), x: tick(2)});
        return log_ * 100 + m.x * 10 + m.y;
    }
}
