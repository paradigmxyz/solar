//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=evm-ir-runtime
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@ filecheck: --implicit-check-not=msize

// OpenZeppelin's `Proxy._delegate`. The `switch` keeps its scrutinee on the stack after
// `returndatacopy` instead of spilling it into the returned data, so the function needs no
// dynamic spill base.
// https://github.com/paradigmxyz/solar/issues/1625

contract Proxy {
    address internal immutable implementation;

    constructor(address target) {
        implementation = target;
    }

    // CHECK: calldatacopy
    // CHECK: delegatecall
    // CHECK: returndatacopy
    // CHECK-NOT: mstore
    // CHECK: revert
    // CHECK: return
    fallback() external payable {
        address target = implementation;
        assembly {
            calldatacopy(0, 0, calldatasize())
            let result := delegatecall(gas(), target, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            switch result
            case 0 { revert(0, returndatasize()) }
            default { return(0, returndatasize()) }
        }
    }
}
