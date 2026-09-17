//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Calls.callInto` copies at most the output buffer's length of what the
// callee returns, and reports both how much arrived and how much there was.
// A reverting callee gives `false` with its revert data in the buffer. The
// intrinsic is the `call` itself at the call site, its three results handed
// back as values; the shipped body is a function that stages two of them in
// memory on the way out.
// INTRINSIC-LABEL: fn @probe
// INTRINSIC-NOT: icall @callInto
// INTRINSIC: = call {{v[0-9]+}}, {{v[0-9]+}}, 0,
// INTRINSIC: returndatasize
// INTRINSIC-NOT: icall @callInto
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

    function probeDelegate(bytes memory initcode)
        public
        returns (bool ok, uint256 copied, uint256 total, bytes32 first)
    {
        address target = Create.deploy(initcode, 0);
        bytes memory out = new bytes(32);
        (ok, copied, total) = Calls.delegateCallInto(target, gasleft(), "", out);
        first = Bytes.readBytes32(out, 0);
    }

    function probeWide(bytes memory initcode)
        public
        returns (bool ok, uint256 copied, uint256 total, bytes32 second, bytes32 third)
    {
        address target = Create.deploy(initcode, 0);
        bytes memory out = new bytes(96);
        Bytes.fill(out, 0, 96, 0xff);
        (ok, copied, total) = Calls.callInto(target, 0, gasleft(), "", out);
        second = Bytes.readBytes32(out, 32);
        third = Bytes.readBytes32(out, 64);
    }

    function probeEmpty() public returns (bool ok, uint256 copied, uint256 total, bytes32 first) {
        bytes memory out = new bytes(32);
        Bytes.fill(out, 0, 32, 0xff);
        (ok, copied, total) = Calls.callInto(address(0xdead), 0, gasleft(), "", out);
        first = Bytes.readBytes32(out, 0);
    }
}
