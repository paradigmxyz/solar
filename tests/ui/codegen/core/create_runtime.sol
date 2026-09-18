//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: spawn 0x63deadbeef6000526004601cf3 => 0xdeadbeef
//@ run-call: spawnAt 0x63deadbeef6000526004601cf3, 0x0000000000000000000000000000000000000000000000000000000000000001 => 0xdeadbeef
//@ run-call: predicted 0x63deadbeef6000526004601cf3, 0x0000000000000000000000000000000000000000000000000000000000000002 => true
//@ run-call-fail: spawn 0xfe => 0x30116425
//@ run-call-fail: spawnAt 0xfe, 0x0000000000000000000000000000000000000000000000000000000000000003 => 0x30116425
//@ run-call: attempt 0x63deadbeef6000526004601cf3 => true, 0xdeadbeef
//@ run-call: attempt 0xfe => false, 0x
//@ run-call: attemptAt 0x63deadbeef6000526004601cf3, 0x0000000000000000000000000000000000000000000000000000000000000004 => true, 0xdeadbeef
//@ run-call: attemptAt 0xfe, 0x0000000000000000000000000000000000000000000000000000000000000005 => false, 0x
//@ run-call: twice 0x63deadbeef6000526004601cf3, 0x0000000000000000000000000000000000000000000000000000000000000006 => true, false

// `Create.deploy` and `deploy2` run whatever initcode they are given and
// revert with `DeploymentFailed()` when creation returns no address; the
// initcode here returns four bytes of runtime code, or is `INVALID`.
// `predict2` is arithmetic and must agree with what `deploy2` did. The `try`
// variants report the failure instead: `false` and the zero address.
import {Create} from "solar:core/v1/Create.sol";

contract Test {
    function spawn(bytes memory initcode) public returns (bytes memory) {
        return Create.deploy(initcode, 0).code;
    }

    function spawnAt(bytes memory initcode, bytes32 salt) public returns (bytes memory) {
        return Create.deploy2(initcode, salt, 0).code;
    }

    function predicted(bytes memory initcode, bytes32 salt) public returns (bool) {
        address deployed = Create.deploy2(initcode, salt, 0);
        return deployed == Create.predict2(address(this), salt, keccak256(initcode));
    }

    function attempt(bytes memory initcode) public returns (bool ok, bytes memory code) {
        address deployed;
        (ok, deployed) = Create.tryDeploy(initcode, 0);
        code = deployed.code;
    }

    function attemptAt(bytes memory initcode, bytes32 salt) public returns (bool ok, bytes memory code) {
        address deployed;
        (ok, deployed) = Create.tryDeploy2(initcode, salt, 0);
        code = deployed.code;
    }

    // The second creation at the same address collides and fails.
    function twice(bytes memory initcode, bytes32 salt) public returns (bool first, bool second) {
        (first,) = Create.tryDeploy2(initcode, salt, 0);
        (second,) = Create.tryDeploy2(initcode, salt, 0);
    }
}
