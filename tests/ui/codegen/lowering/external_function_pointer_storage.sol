//@ run-call: Flow::f() => 1, 2
// ported-from: test/libsolidity/semanticTests/functionTypes/struct_with_external_function.sol

struct S {
    uint16 a;
    function() external returns (uint256) pointer;
    uint16 b;
}

contract Flow {
    S[2] values;

    function first() public pure returns (uint256) {
        return 1;
    }

    function second() public pure returns (uint256) {
        return 2;
    }

    constructor() {
        values[0].a = 0xff07;
        values[0].b = 0xff07;
        values[1].pointer = this.second;
        values[1].a = 0xff07;
        values[1].b = 0xff07;
        values[0].pointer = this.first;
    }

    function f() public returns (uint256, uint256) {
        return (values[0].pointer(), values[1].pointer());
    }
}
