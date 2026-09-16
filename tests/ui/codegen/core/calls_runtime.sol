//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: probe 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3 => true, 32, 64, 0x1111111111111111111111111111111111111111111111111111111111111111
//@ run-call: probe 0x6029600c60003960296000f37f333333333333333333333333333333333333333333333333333333333333333360005260206000fd => false, 32, 32, 0x3333333333333333333333333333333333333333333333333333333333333333
//@ run-call: probeStatic 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3 => true, 32, 64, 0x1111111111111111111111111111111111111111111111111111111111111111

// `Calls.callInto` copies at most the output buffer's length of what the
// callee returns, and reports both how much arrived and how much there was.
// A reverting callee gives `false` with its revert data in the buffer. The
// three-value return keeps this a shipped body for now.
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
