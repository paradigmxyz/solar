//@ compile-flags: -Ogas -Zmir-pipeline=none -Zdump=mir
//@ filecheck:

import {Abi} from "solar:core/v1/Abi.sol";

contract Test {
    // An encoding written as the argument is staged past the free memory
    // pointer, range-checked against `out`, and copied from there.
    // CHECK-LABEL: fn @place(
    // CHECK: abi_encode [word, memory_bytes], scratch
    // CHECK: panic_if<0x32>
    // CHECK: mcopy
    // CHECK: ret
    function place(bytes memory out, uint256 a, bytes memory b) public pure returns (uint256) {
        return Abi.writeEncoding(out, 0, abi.encode(a, b));
    }

    // Any other argument is copied from the object it names.
    // CHECK-LABEL: fn @copied(
    // CHECK: abi_encode [word, memory_bytes], object
    // CHECK: mcopy
    function copied(bytes memory out, uint256 a, bytes memory b) public pure returns (uint256) {
        bytes memory encoding = abi.encode(a, b);
        return Abi.writeEncoding(out, 0, encoding);
    }
}
