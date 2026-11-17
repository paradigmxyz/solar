//@ revisions: none size
//@[none] compile-flags: -Onone -Zdump=evm-ir-runtime
//@[size] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefix=SIZE
//@ normalize-stdout-test: "(?s).+" -> ""

// The call statement that ends an initializer after a `require` is a tail call. An unoptimized
// build keeps its jump in a block of its own, so that the `require` branches to the statement and
// not past it to the callee, and the statement has an instruction.

// CHECK-LABEL: (runtime) ===
// CHECK: [invoke=__init@
// CHECK: jumpi
// CHECK: bb{{[0-9]+}}:
// CHECK-NEXT: push bb{{[0-9]+}} !metadata(keep_with_next)
// CHECK-NEXT: jump

// SIZE-LABEL: (runtime) ===
// SIZE-NOT: keep_with_next
contract C {
    error Zero();
    address public a;

    function init(address x) external {
        __init(x);
    }

    function __init(address x) internal {
        require(x != address(0), Zero());
        _set(x);
    }

    function _set(address x) internal {
        a = x;
    }
}
