//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: part 0x63deadbeef6000526004601cf3, 0, 4 => 0xdeadbeef
//@ run-call: part 0x63deadbeef6000526004601cf3, 1, 2 => 0xadbe
//@ run-call-fail: part 0x63deadbeef6000526004601cf3, 2, 4 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// Reading a range of another account's code, checked against its size.
// CHECK-LABEL: fn @part
// CHECK: extcodecopy
// CHECK-NOT: icall @copyInto
import {Code} from "solar:core/v1/Code.sol";
import {Create} from "solar:core/v1/Create.sol";

contract Safe {
    function part(bytes memory initcode, uint256 start, uint256 count) public returns (bytes memory out) {
        out = new bytes(count);
        Code.copyInto(out, 0, Create.deploy(initcode, 0), start, count);
    }
}
