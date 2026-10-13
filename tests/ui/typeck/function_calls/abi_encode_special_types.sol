// ported-from: test/libsolidity/syntaxTests/specialFunctions/types_with_unspecified_encoding_special_types.sol

contract C {
    function f() public pure {
        (bool a,) = address(this).call(abi.encode(address(this).delegatecall, super));
        //~^ ERROR: `encode` argument cannot be ABI-encoded
        //~| ERROR: `encode` argument cannot be ABI-encoded
        (a,) = address(this).delegatecall(abi.encode(block, tx, mulmod));
        //~^ ERROR: `encode` argument cannot be ABI-encoded
        //~| ERROR: `encode` argument cannot be ABI-encoded
        //~| ERROR: `encode` argument cannot be ABI-encoded
        a;
    }
}
