// Only plain external function values can be ABI-encoded.

library L {
    function pub(uint256) public pure {}
}

contract D {}

contract C {
    function f() public {
        abi.encode(this.f);
        abi.encode(new D); //~ ERROR: `encode` argument cannot be ABI-encoded
        abi.encode(new D{salt: 0}); //~ ERROR: `encode` argument cannot be ABI-encoded
        abi.encode(L.pub); //~ ERROR: `encode` argument cannot be ABI-encoded
        abi.encodeWithSelector(bytes4(0), this.f{gas: 1}); //~ ERROR: `encodeWithSelector` argument cannot be ABI-encoded
    }
}
