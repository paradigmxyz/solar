//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: ConstructorMemoryReturn::value() => "1"
//@[none, gas, size] run-call: ConstructorMemoryReturn::direct() => 5
//@[none, gas, size] run-call: ConstructorMemoryReturn::pairValue() => 3

contract ConstructorMemoryReturn {
    string public value;

    struct Pair {
        uint256 x;
        uint256 y;
    }

    constructor() {
        value = consume(version());
    }

    function version() public pure returns (string memory) {
        return "1";
    }

    function consume(string memory input) internal pure returns (string memory) {
        return input;
    }

    function direct() public pure returns (uint256) {
        return bytes(consume("hello")).length;
    }

    function makePair(uint256 x) internal pure returns (Pair memory) {
        return Pair({x: x, y: x + 1});
    }

    function pairValue() public pure returns (uint256) {
        return makePair(2).y;
    }
}
