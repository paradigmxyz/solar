//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Calls.forwardDelegate` ends the call on every path, so it stages the
// calldata at the start of memory, as proxies do in assembly, delegate-calls
// with no output buffer, and copies the response there to return or revert
// with. The shipped body is a function doing the same past the free memory
// pointer.
// INTRINSIC-LABEL: fn @fallback
// INTRINSIC: calldatacopy 0, 0, [[SIZE:v[0-9]+]]
// INTRINSIC: {{v[0-9]+}} = delegatecall {{v[0-9]+}}, {{v[0-9]+}}, 0, [[SIZE]], 0, 0
// INTRINSIC-DAG: revert 0, {{v[0-9]+}}
// INTRINSIC-DAG: returndata 0, {{v[0-9]+}}
// PORTABLE-LABEL: fn @fallback
// PORTABLE: delegatecall
import {Calls} from "solar:core/v1/Calls.sol";

contract Test {
    address private immutable implementation = address(this);

    fallback() external payable {
        Calls.forwardDelegate(implementation, msg.data);
    }
}
