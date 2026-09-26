//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: 0xda91254c => 0x00000000000000000000000022222222222222222222222222222222222222220000000000000000000000000000000000000000000000000000000000000001
//@ run-call: 0xcfae3217 => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001668656c6c6f2c20666f7277617264656420776f726c6400000000000000000000
//@ run-call-fail: 0x132e4f3c0000000000000000000000000000000000000000000000000000000000000007 => 0xc77ea6410000000000000000000000000000000000000000000000000000000000000007

// The same proxy in assembly, as the proxy libraries write it: the calldata
// goes to the start of memory, over the free memory pointer, and nothing
// stops the block from ending a call to a function that declares an ABI
// output, or from skipping a modifier's code after `_`.
// CHECK-LABEL: fn @fallback
// CHECK: calldatacopy 0, 0, {{v[0-9]+}}
// CHECK: delegatecall {{v[0-9]+}}, {{v[0-9]+}}, 0, {{v[0-9]+}}, 0, 0
import {Target} from "./auxiliary/target.sol";

contract Unsafe {
    address private immutable implementation = address(new Target());

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
