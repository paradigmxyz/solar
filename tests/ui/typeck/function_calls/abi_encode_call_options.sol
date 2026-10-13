// ported-from: test/libsolidity/syntaxTests/specialFunctions/functionCallOptions_err.sol

contract C {
    function f() public payable {
		abi.encode(this.f{value: 2}); //~ ERROR: `encode` argument cannot be ABI-encoded
		abi.encode(this.f{gas: 2}); //~ ERROR: `encode` argument cannot be ABI-encoded
		abi.encode(this.f{value: 2, gas: 1}); //~ ERROR: `encode` argument cannot be ABI-encoded
    }
}
