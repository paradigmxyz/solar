//@ compile-flags: --emit=ast
//@ filecheck:

// Compact output writes each `typeDescriptions` once and copies it after. Expressions of the same
// type that print differently must not share it: literals, tuples, call options and builtins.
// CHECK: "typeString":"int_const 2"
// CHECK: "typeString":"int_const 3"
// CHECK: "typeString":"literal_string \"ab\""
// CHECK: "typeString":"literal_string \"cd\""
// CHECK: "typeString":"tuple(int_const 2,int_const 3)"
// CHECK: "typeString":"tuple(int_const 3,int_const 2)"
// CHECK: "typeIdentifier":"t_function_external_payable$__$returns$__$value"
// CHECK: "typeIdentifier":"t_function_external_payable$__$returns$__$gas"
// CHECK: "typeIdentifier":"t_function_keccak256_pure$
// CHECK: "typeIdentifier":"t_function_sha256_pure$
contract C {
    function f() external payable {}

    function g() external returns (uint256, uint256, bytes32, bytes32) {
        uint256 a = 2;
        uint256 b = 3;
        string memory s = "ab";
        s = "cd";
        (a, b) = (2, 3);
        (a, b) = (3, 2);
        this.f{value: 1}();
        this.f{gas: 1}();
        return (a, b, keccak256(bytes(s)), sha256(bytes(s)));
    }
}
