//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: whole 0x63deadbeef6000526004601cf3 => 0xdeadbeef
//@ run-call: part 0x63deadbeef6000526004601cf3, 1, 2 => 0xadbe
//@ run-call: part 0x63deadbeef6000526004601cf3, 4, 0 => 0x
//@ run-call: patched 0x63deadbeef6000526004601cf3 => 0xaadeadbeefaa
//@ run-call-fail: part 0x63deadbeef6000526004601cf3, 2, 4 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: part 0x63deadbeef6000526004601cf3, 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: overflow 0x63deadbeef6000526004601cf3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// `Code.copyInto` copies a range of another account's code into a buffer,
// checked against the code's size and the buffer's, so nothing is padded and
// nothing lands outside. `read` is library code over it.
import {Code} from "solar:core/v1/Code.sol";
import {Create} from "solar:core/v1/Create.sol";

contract Test {
    function whole(bytes memory initcode) public returns (bytes memory) {
        address target = Create.deploy(initcode, 0);
        return Code.read(target, 0, target.code.length);
    }

    function part(bytes memory initcode, uint256 start, uint256 count) public returns (bytes memory) {
        return Code.read(Create.deploy(initcode, 0), start, count);
    }

    function patched(bytes memory initcode) public returns (bytes memory out) {
        out = hex"aaaaaaaaaaaa";
        Code.copyInto(out, 1, Create.deploy(initcode, 0), 0, 4);
    }

    function overflow(bytes memory initcode) public returns (bytes memory out) {
        out = new bytes(3);
        Code.copyInto(out, 0, Create.deploy(initcode, 0), 0, 4);
    }
}
