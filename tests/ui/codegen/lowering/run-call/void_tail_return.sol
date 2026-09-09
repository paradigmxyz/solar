//@ codegen-matrix: standard
//@ run-call: once 2, 5 => 522
//@ run-call: twice 2, 5 => 522255
//@ run-call: once 5, 2 => 255
//@ run-call-fail: once 0, 5
//@ run-call: zero 7 => 23
//@ run-call-fail: zero 0
//@ run-call: recurse 4 => 10
//@ run-call: tuple 2, 5 => 2, 5, 5
//@ run-call-fail: tuple 0, 5
//@ run-call-fail: tuple 2, 0

contract VoidTailReturn {
    uint256 private total;
    uint256 private second;
    uint256 private third;

    function tuple(uint256 a, uint256 b) external returns (uint256, uint256, uint256) {
        forwardTuple(a, b);
        forwardTuple(b, a);
        return (total, second, third);
    }

    // Keep an arithmetic-free path for symbolic comparison over full words.
    function forwardTuple(uint256 a, uint256 b) internal {
        require(a != 0);
        tupleSink(b, a, a);
    }

    function tupleSink(uint256 a, uint256 b, uint256 c) internal {
        require(a != 0);
        total = a;
        second = b;
        third = c;
    }

    function once(uint256 a, uint256 b) external returns (uint256) {
        forward(a, b);
        return total;
    }

    function twice(uint256 a, uint256 b) external returns (uint256) {
        forward(a, b);
        forward(b, a);
        return total;
    }

    // The terminal call permutes arguments and repeats one actual. Its return
    // must resume the original caller, including the second call in `twice`.
    function forward(uint256 a, uint256 b) internal {
        require(a != 0);
        sink(b, a, a);
    }

    function sink(uint256 a, uint256 b, uint256 c) internal {
        require(a < 10 && b < 10 && c < 10);
        total = total * 1000 + a * 100 + b * 10 + c;
    }

    function zero(uint256 a) external returns (uint256) {
        zeroForward(a);
        return total;
    }

    // A zero-argument transfer must drop any caller words above the inherited
    // return address. A loop keeps the callee available as a shared function.
    function zeroForward(uint256 a) internal {
        require(a != 0);
        total = a;
        finish();
    }

    function finish() internal {
        for (uint256 i; i < 4; ++i) total += i + 1;
        total += 6;
    }

    function recurse(uint256 depth) external returns (uint256) {
        recursive(depth);
        return total;
    }

    // Recursive frames retain their existing call protocol.
    function recursive(uint256 depth) internal {
        total += depth;
        if (depth != 0) recursive(depth - 1);
    }
}
