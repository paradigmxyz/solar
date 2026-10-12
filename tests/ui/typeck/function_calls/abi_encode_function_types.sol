// Only external function values can be ABI-encoded.

library L {
    function g() public {}
}

contract C {
    function f() public payable {}
    function i() internal {}

    function t() public view returns (bytes memory) {
        abi.encode(assert); //~ ERROR: argument cannot be ABI-encoded
        abi.encode(gasleft); //~ ERROR: argument cannot be ABI-encoded
        abi.encodePacked(keccak256); //~ ERROR: argument cannot be ABI-encoded
        abi.encodeWithSelector(0x12345678, blockhash); //~ ERROR: argument cannot be ABI-encoded
        abi.encodeWithSignature("x()", ecrecover); //~ ERROR: argument cannot be ABI-encoded
        abi.encode(i); //~ ERROR: argument cannot be ABI-encoded
        abi.encode(C.f); //~ ERROR: argument cannot be ABI-encoded
        abi.encode(L.g); //~ ERROR: argument cannot be ABI-encoded
        abi.encode(address(this).call); //~ ERROR: argument cannot be ABI-encoded
        return abi.encode(this.f);
    }
}
