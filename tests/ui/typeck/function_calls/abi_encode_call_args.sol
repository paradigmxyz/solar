//@compile-flags: -Ztypeck

contract C {
    function f1(int256 value) public {
        value;
    }

    function f2(int256 a, int256 b) public {
        a;
        b;
    }

    function returnsPair() internal pure returns (uint256, uint256) {
        return (1, 2);
    }

    function test() public returns (bytes memory) {
        abi.encodeCall(this.f1, ("test")); //~ ERROR: mismatched types
        abi.encodeCall(this.f1, (1, 2)); //~ ERROR: wrong argument count for `abi.encodeCall`
        abi.encodeCall(this.f1, ()); //~ ERROR: wrong argument count for `abi.encodeCall`
        abi.encodeCall(this.f1); //~ ERROR: wrong argument count for function call

        abi.encodeCall(this.f2, [1, 2]); //~ ERROR: wrong argument count for `abi.encodeCall`
        //~^ ERROR: mismatched types
        abi.encodeCall(this.f2, ((1, 2))); //~ ERROR: wrong argument count for `abi.encodeCall`
        //~^ ERROR: mismatched types
        abi.encodeCall(this.f1, (1,)); //~ ERROR: tuple components cannot be empty
        abi.encodeCall(this.f2, (, 1)); //~ ERROR: tuple components cannot be empty
        abi.encodeCall(this.f2, returnsPair()); //~ ERROR: second argument to `abi.encodeCall` must be an inline tuple

        return "";
    }
}
