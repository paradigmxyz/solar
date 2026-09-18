//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Hash.keccak256Range` hashes a range of a buffer. The shipped body copies
// the range out and hashes the copy; the intrinsic hashes it where it lies.
// Both check the range and fail the same way when it does not fit.
// INTRINSIC-LABEL: fn @hash
// INTRINSIC: keccak256 {{v[0-9]+}}, arg2
// INTRINSIC-NOT: mstore8
// PORTABLE-LABEL: fn @hash
// PORTABLE: mstore8
import {Hash} from "solar:core/v1/Hash.sol";

contract Test {
    function hash(bytes memory b, uint256 offset, uint256 count) public pure returns (bytes32) {
        return Hash.keccak256Range(b, offset, count);
    }
}
