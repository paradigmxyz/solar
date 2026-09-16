//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Calls.callInto` copies at most the output buffer's length of what the
// callee returns, and reports both how much arrived and how much there was.
// A reverting callee gives `false` with its revert data in the buffer. The
// three-value return keeps this a shipped body for now, so both revisions
// call the same function: one `call` and one `returndatasize`.
// INTRINSIC-LABEL: fn @callInto
// INTRINSIC: call arg2, arg0, arg1
// INTRINSIC: returndatasize
// PORTABLE-LABEL: fn @callInto
// PORTABLE: call arg2, arg0, arg1
// PORTABLE: returndatasize
import {Bytes} from "solar:core/v1/Bytes.sol";
import {Calls} from "solar:core/v1/Calls.sol";
import {Create} from "solar:core/v1/Create.sol";

contract Test {
    function probe(bytes memory initcode)
        public
        returns (bool ok, uint256 copied, uint256 total, bytes32 first)
    {
        address target = Create.deploy(initcode, 0);
        bytes memory out = new bytes(32);
        (ok, copied, total) = Calls.callInto(target, 0, gasleft(), "", out);
        first = Bytes.readBytes32(out, 0);
    }

    function probeStatic(bytes memory initcode)
        public
        returns (bool ok, uint256 copied, uint256 total, bytes32 first)
    {
        address target = Create.deploy(initcode, 0);
        bytes memory out = new bytes(32);
        (ok, copied, total) = Calls.staticCallInto(target, gasleft(), "", out);
        first = Bytes.readBytes32(out, 0);
    }
}
