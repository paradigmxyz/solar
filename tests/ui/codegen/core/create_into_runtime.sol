//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: attempt 0x63deadbeef6000526004601cf3, 8 => true, 0xdeadbeef, 0, 0, 0xffffffffffffffff
//@ run-call: attempt 0x7f333333333333333333333333333333333333333333333333333333333333333360005260206000fd, 4 => false, 0x, 4, 32, 0x33333333
//@ run-call: attempt 0x7f333333333333333333333333333333333333333333333333333333333333333360005260206000fd, 40 => false, 0x, 32, 32, 0x3333333333333333333333333333333333333333333333333333333333333333ffffffffffffffff
//@ run-call: attempt 0x7f333333333333333333333333333333333333333333333333333333333333333360005260206000fd, 0 => false, 0x, 0, 32, 0x
//@ run-call: attempt 0xfe, 8 => false, 0x, 0, 0, 0xffffffffffffffff

// `Create.tryDeployInto` hands back what a failed constructor reverted with,
// bounded by the buffer the caller owns: how much arrived, how much there was,
// and the rest of the buffer untouched. A creation that succeeds, and one that
// fails without data, copy nothing.
import {Bytes} from "solar:core/v1/Bytes.sol";
import {Create} from "solar:core/v1/Create.sol";

contract Test {
    function attempt(bytes memory initcode, uint256 room)
        public
        returns (bool ok, bytes memory code, uint256 copied, uint256 total, bytes memory diagnostics)
    {
        diagnostics = new bytes(room);
        Bytes.fill(diagnostics, 0, room, 0xff);
        address deployed;
        (ok, deployed, copied, total) = Create.tryDeployInto(initcode, 0, diagnostics);
        code = deployed.code;
    }
}
