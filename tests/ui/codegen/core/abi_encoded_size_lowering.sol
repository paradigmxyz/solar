//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Abi.encodedSize(abi.encode(...))` of values is a constant, and of byte
// strings and arrays of values arithmetic on their lengths: nothing is
// encoded or allocated. An array of byte strings is encoded past the free
// memory pointer, without moving it, and measured. The shipped body allocates
// the encoding and reads its length.
// INTRINSIC-LABEL: fn @fixedSize
// INTRINSIC-NOT: mload 64
// INTRINSIC: mstore {{[0-9]+}}, 96
// INTRINSIC-LABEL: fn @dynamicSize
// INTRINSIC-NOT: mload 64
// INTRINSIC: returndata {{[0-9]+}}, 32
// INTRINSIC-LABEL: fn @staged
// INTRINSIC-NOT: mstore 64
// INTRINSIC: mload 64
// INTRINSIC-NOT: mstore 64
// INTRINSIC: returndata {{[0-9]+}}, 32
// PORTABLE-LABEL: fn @fixedSize
// PORTABLE: mstore 64
import {Abi} from "solar:core/v1/Abi.sol";

contract Test {
    function fixedSize(uint256 a, address b) public pure returns (uint256) {
        return Abi.encodedSize(abi.encode(a, b, true));
    }

    function dynamicSize(bytes calldata b, uint256[] calldata xs) public pure returns (uint256) {
        return Abi.encodedSize(abi.encodeWithSelector(0x12345678, b, xs));
    }

    function staged(bytes[] calldata items) public pure returns (uint256) {
        return Abi.encodedSize(abi.encode(items));
    }
}
