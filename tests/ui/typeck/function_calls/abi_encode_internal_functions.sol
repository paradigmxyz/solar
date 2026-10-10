// ported-from: test/libsolidity/syntaxTests/specialFunctions/types_with_unspecified_encoding_internal_functions.sol

contract C {
    function f() payable public {
        bytes32 h = keccak256(abi.encodePacked(keccak256, f, this.f{value: 2}, blockhash));
        //~^ ERROR: `encodePacked` argument cannot be ABI-encoded
        //~| ERROR: `encodePacked` argument cannot be ABI-encoded
        //~| ERROR: `encodePacked` argument cannot be ABI-encoded
        //~| ERROR: `encodePacked` argument cannot be ABI-encoded
        h;
    }
}
