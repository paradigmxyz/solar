//@ revisions: intrinsic portable size
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[size] compile-flags: -Osize
//@ run-call: 0xcad0899b00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003 => 0x0000000000000000000000000000000000000000000000000000000000000005
//@ run-call: 0xda91254c => 0x00000000000000000000000022222222222222222222222222222222222222220000000000000000000000000000000000000000000000000000000000000001
//@ run-call: 0x295b4e17; value=3 => 0x0000000000000000000000000000000000000000000000000000000000000003
//@ run-call: 0xcfae3217 => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001668656c6c6f2c20666f7277617264656420776f726c6400000000000000000000
//@ run-call-fail: 0x132e4f3c0000000000000000000000000000000000000000000000000000000000000007 => 0xc77ea6410000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: 0x => 0x

// `Calls.forward` passes the whole calldata and value on and ends the call
// with the callee's return data, or with its revert data as the revert.
// `Calls.forwardDelegate` runs the callee on this contract's storage with the
// original sender, so `whoAmI` counts in the proxy and sees the caller. A
// target with no fallback rejects empty calldata with no revert data, which
// comes back as it is.
import {Calls} from "solar:core/Calls.sol";
import {ForwardTarget} from "./auxiliary/forward_target.sol";

contract Proxy {
    // Storage slot zero stays free for the delegated counter.
    address private immutable target = address(new ForwardTarget());

    fallback() external payable {
        if (msg.sig == ForwardTarget.whoAmI.selector) Calls.forwardDelegate(target, msg.data);
        Calls.forward(target, msg.value, msg.data);
    }
}
