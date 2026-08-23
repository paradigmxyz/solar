//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: f((function)) (0x303132333435363738393031323334353637383961626364) => 1
//@ run-call-fail: 0x9f4a608d3031323334353637383930313233343536373839616263645800000000000000
//@ run-call: g((function)) (0x303132333435363738393031323334353637383961626364) => 2
//@ run-call: 0x5cf6281d3031323334353637383930313233343536373839616263645800000000000000 => 0x0000000000000000000000000000000000000000000000000000000000000002
//@ run-call: h((function)) (0x303132333435363738393031323334353637383961626364) => 3
//@ run-call-fail: 0x4fc262ac3031323334353637383930313233343536373839616263645800000000000000
// ported-from: test/libsolidity/semanticTests/abicoder/validation/external_function_type_inside_struct_v2.sol

pragma abicoder v2;

contract AbiFunctionPointerValidation {
    struct S {
        function() external x;
    }

    function f(S memory) external pure returns (uint r) {
        r = 1;
    }

    function g(S calldata) external pure returns (uint r) {
        r = 2;
    }

    function h(S calldata s) external pure returns (uint r) {
        s.x;
        r = 3;
    }
}
