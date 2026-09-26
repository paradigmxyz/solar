//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Code.copyInto` copies a range of another account's code into a buffer,
// checked against the code's size and the buffer's, so nothing is padded and
// nothing lands outside. `read` copies into a new buffer the same way, without
// zeroing it first, and clears only the word after the bytes; its check shares
// a size read of the same account.
// INTRINSIC-LABEL: fn @whole
// INTRINSIC: extcodesize
// INTRINSIC-NOT: extcodesize
// INTRINSIC-NOT: calldatacopy
// INTRINSIC-NOT: icall @read
// INTRINSIC: extcodecopy
// PORTABLE-LABEL: fn @whole
// PORTABLE: icall @read
// INTRINSIC-LABEL: fn @patched
// INTRINSIC: extcodesize
// INTRINSIC: extcodecopy
// INTRINSIC-NOT: icall @copyInto
// PORTABLE-LABEL: fn @patched
// PORTABLE: extcodecopy
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
