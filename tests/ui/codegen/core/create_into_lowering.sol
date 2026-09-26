//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// The intrinsic is the `create`, the size of what came back, the smaller of
// that and the buffer, and one `returndatacopy`: no branch, since a creation
// that succeeds leaves no return data to copy.
// INTRINSIC-LABEL: fn @attempt
// INTRINSIC: create
// INTRINSIC: returndatasize
// INTRINSIC: returndatacopy
// INTRINSIC-NOT: icall @tryDeployInto
// PORTABLE-LABEL: fn @tryDeployInto
// PORTABLE: returndatacopy
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
