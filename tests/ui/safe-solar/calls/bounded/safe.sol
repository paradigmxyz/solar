//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: probe 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3 => true, 32, 64, 0x1111111111111111111111111111111111111111111111111111111111111111
//@ run-call: probe 0x6029600c60003960296000f37f333333333333333333333333333333333333333333333333333333333333333360005260206000fd => false, 32, 32, 0x3333333333333333333333333333333333333333333333333333333333333333

// A call whose output lands in a buffer the caller owns: at most the buffer's
// length is copied, and the caller learns both how much arrived and how much
// there was. A reverting callee is `false` with its data in the buffer.
// CHECK-LABEL: fn @probe
// CHECK-NOT: icall @callInto
// CHECK: = call {{v[0-9]+}}, {{v[0-9]+}}, 0,
// CHECK: returndatasize
import {Bytes} from "solar:core/v1/Bytes.sol";
import {Calls} from "solar:core/v1/Calls.sol";
import {Create} from "solar:core/v1/Create.sol";

contract Safe {
    function probe(bytes memory initcode)
        public
        returns (bool ok, uint256 copied, uint256 total, bytes32 first)
    {
        address target = Create.deploy(initcode, 0);
        bytes memory out = new bytes(32);
        (ok, copied, total) = Calls.callInto(target, 0, gasleft(), "", out);
        first = Bytes.readBytes32(out, 0);
    }
}
