//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: b => 123
//@ run-call: c => 123
//@ run-call: d => 7
//@ run-call: structNamed => 1221
//@ run-call: structPositional => 1212
//@ run-call: callNamed => 1221
//@ run-call: callPositional => 1212
//@ run-call: externalNamed => 12
//@ run-call: memoryStructNamed => 1221
//@ run-call: eventNamed => 12
// ported-from: test/libsolidity/semanticTests/functionCall/named_args.sol

contract NamedArguments {
    uint256 log;

    event Named(uint256 x, uint256 y);

    struct Pair {
        uint256 x;
        uint256 y;
    }

    function a(uint a, uint b, uint c) internal pure returns (uint r) {
        r = a * 100 + b * 10 + c;
    }
    function b() external pure returns (uint r) {
        r = a({a: 1, b: 2, c: 3});
    }
    function c() external pure returns (uint r) {
        r = a({b: 2, c: 3, a: 1});
    }

    modifier cap(uint x) {
        require(x > 0);
        _;
    }
    function d() external pure cap({x: 5}) returns (uint) {
        return a({c: 7, a: 0, b: 0});
    }

    function tick(uint256 tag) internal returns (uint256) {
        log = log * 10 + tag;
        return tag;
    }

    function take(uint256 x, uint256 y) internal pure returns (uint256) {
        return x * 10 + y;
    }

    function structNamed() external returns (uint256) {
        pairs = Pair({y: tick(1), x: tick(2)});
        return log * 100 + pairs.x * 10 + pairs.y;
    }

    function structPositional() external returns (uint256) {
        pairs = Pair(tick(1), tick(2));
        return log * 100 + pairs.x * 10 + pairs.y;
    }

    function callNamed() external returns (uint256) {
        uint256 result = take({y: tick(1), x: tick(2)});
        return log * 100 + result;
    }

    function callPositional() external returns (uint256) {
        uint256 result = take(tick(1), tick(2));
        return log * 100 + result;
    }

    function externalNamed() external returns (uint256) {
        this.receivePair({y: tick(1), x: tick(2)});
        return log;
    }

    function receivePair(uint256 x, uint256 y) external pure {
        x;
        y;
    }

    function memoryStructNamed() external returns (uint256) {
        Pair memory pair = Pair({y: tick(1), x: tick(2)});
        return log * 100 + pair.x * 10 + pair.y;
    }

    function eventNamed() external returns (uint256) {
        emit Named({y: tick(1), x: tick(2)});
        return log;
    }

    Pair pairs;
}
