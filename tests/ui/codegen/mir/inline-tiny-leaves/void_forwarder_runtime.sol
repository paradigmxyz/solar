//@ codegen-matrix: standard
//@ run-call: once 2, 13 => 2137
//@ run-call: twice 2, 13 => 226727
//@ run-call-fail: once 2, 0
//@ run-call: wrap 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 3

contract VoidForwarder {
    uint256 private total;

    function once(uint256 x, uint256 y) external returns (uint256) {
        forward(x, y, 7);
        return total;
    }

    function twice(uint256 x, uint256 y) external returns (uint256) {
        forward(x, y, 7);
        forward(y, x, 7);
        return total;
    }

    function wrap(uint256 x) external returns (uint256) {
        wrappingForward(x);
        return total;
    }

    function forward(uint256 x, uint256 y, uint256 z) internal {
        sink(y, x, z);
    }

    function wrappingForward(uint256 x) internal {
        uncheckedSink(x, 4);
    }

    function sink(uint256 x, uint256 y, uint256 z) internal {
        require(x != 0);
        total = total * 100 + y * 1000 + x * 10 + z;
    }

    function uncheckedSink(uint256 x, uint256 y) internal {
        if (x == 0) revert();
        unchecked { total = x + y; }
    }
}
