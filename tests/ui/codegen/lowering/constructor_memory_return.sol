//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: ConstructorMemoryReturn::value() => "1"

contract ConstructorMemoryReturn {
    string public value;

    constructor() {
        value = consume(version());
    }

    function version() public pure returns (string memory) {
        return "1";
    }

    function consume(string memory input) internal pure returns (string memory) {
        return input;
    }
}
