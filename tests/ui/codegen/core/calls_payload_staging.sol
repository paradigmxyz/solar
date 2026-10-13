//@ revisions: intrinsic portable mir
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] compile-flags: -Ogas -Zdump=mir
//@[mir] filecheck:
//@ run-call: staged 5 => true, 36, 0x123456780000000000000000000000000000000000000000000000000000000000000005
//@ run-call: stagedStatic 5, 6 => true, 64, 0x00000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000006
//@ run-call: allocated 5 => true, 36, 0x123456780000000000000000000000000000000000000000000000000000000000000005

// A call's payload built by `abi.encode*` is staged past the free memory
// pointer, without reserving it, when the operands after it only name buffers
// already in memory: nothing then allocates before the call reads it. An output
// buffer that is not such a name keeps the payload in an allocation. The
// identity precompile returns its input, so the output buffer receives the
// payload either way.
// Only the output buffer moves the free memory pointer before a staged call.
// CHECK-LABEL: fn @staged()
// CHECK: mstore 64,
// CHECK-NOT: mstore 64,
// CHECK: = call {{v[0-9]+}}, 4, 0, {{v[0-9]+}}, 36,
// CHECK-LABEL: fn @stagedStatic()
// CHECK: mstore 64,
// CHECK-NOT: mstore 64,
// CHECK: = staticcall {{v[0-9]+}}, 4, {{v[0-9]+}}, 64,
// CHECK-LABEL: fn @allocated()
// CHECK: mstore 64,
// CHECK: mstore 64,
// CHECK: mstore 64,
// CHECK: = call {{v[0-9]+}}, 4, 0, {{v[0-9]+}}, {{v[0-9]+}},
import {Calls} from "solar:core/Calls.sol";

contract CallsPayloadStaging {
    function staged(uint256 a) public returns (bool ok, uint256 copied, bytes memory out) {
        out = new bytes(36);
        (ok, copied,) =
            Calls.callInto(address(4), 0, gasleft(), abi.encodeWithSelector(0x12345678, a), out);
    }

    function stagedStatic(uint256 a, uint256 b)
        public
        view
        returns (bool ok, uint256 copied, bytes memory out)
    {
        out = new bytes(64);
        (ok, copied,) = Calls.staticCallInto(address(4), gasleft(), abi.encode(a, b), out);
    }

    function allocated(uint256 a) public returns (bool ok, uint256 copied, bytes memory out) {
        bytes[] memory buffers = new bytes[](1);
        buffers[0] = new bytes(36);
        (ok, copied,) = Calls.callInto(
            address(4), 0, gasleft(), abi.encodeWithSelector(0x12345678, a), buffers[0]
        );
        out = buffers[0];
    }
}
