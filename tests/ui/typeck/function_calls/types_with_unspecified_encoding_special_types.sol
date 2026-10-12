// ported-from: test/libsolidity/syntaxTests/specialFunctions/types_with_unspecified_encoding_special_types.sol
contract C {
    function f() public pure {
        (bool a,) = address(this).call(abi.encode(address(this).delegatecall, super));
        //~^ ERROR: argument cannot be ABI-encoded
        //~| ERROR: argument cannot be ABI-encoded
        (a,) = address(this).delegatecall(abi.encode(block, tx, mulmod));
        //~^ ERROR: argument cannot be ABI-encoded
        //~| ERROR: argument cannot be ABI-encoded
        //~| ERROR: argument cannot be ABI-encoded
        a;
    }
}
