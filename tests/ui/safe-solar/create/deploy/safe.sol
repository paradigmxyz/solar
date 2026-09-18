//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: spawn 0x63deadbeef6000526004601cf3 => 0xdeadbeef
//@ run-call: spawnAt 0x63deadbeef6000526004601cf3, 0x0000000000000000000000000000000000000000000000000000000000000001 => 0xdeadbeef
//@ run-call-fail: spawn 0xfe => 0x30116425

// Deploying supplied initcode. A creation that returns no address reverts
// with `DeploymentFailed()` instead of handing back the zero address.
// CHECK-LABEL: fn @spawn
// CHECK: create
// CHECK: 0x3011642500000000000000000000000000000000000000000000000000000000
// CHECK-NOT: icall @deploy
import {Create} from "solar:core/v1/Create.sol";

contract Safe {
    function spawn(bytes memory initcode) public returns (bytes memory) {
        return Create.deploy(initcode, 0).code;
    }

    function spawnAt(bytes memory initcode, bytes32 salt) public returns (bytes memory) {
        return Create.deploy2(initcode, salt, 0).code;
    }
}
