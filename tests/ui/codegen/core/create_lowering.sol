//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Create.deploy` and `deploy2` run whatever initcode they are given and
// revert with `DeploymentFailed()` when creation returns no address; the
// initcode here returns four bytes of runtime code, or is `INVALID`.
// `predict2` is arithmetic and must agree with what `deploy2` did. The `try`
// variants hand the failure back as a flag. Every throwing site shares one
// four-byte revert.
// INTRINSIC-LABEL: fn @spawn
// INTRINSIC: create
// INTRINSIC: tail_call @revert_stub
// INTRINSIC-NOT: icall @deploy
// INTRINSIC-LABEL: fn @attempt
// INTRINSIC: create
// INTRINSIC-NOT: icall @tryDeploy
// INTRINSIC-LABEL: fn @revert_stub
// INTRINSIC: mstore 0, 0x3011642500000000000000000000000000000000000000000000000000000000
// INTRINSIC-NEXT: revert 0, 4
// PORTABLE-LABEL: fn @spawn
// PORTABLE: create
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
