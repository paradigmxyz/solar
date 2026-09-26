//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Return.abiEncoded` writes the offset word below the string and a zero word
// after its bytes, then returns the range in place, as a function returning
// the string does. The shipped body encodes a copy and returns that.
// INTRINSIC-LABEL: fn @echo
// INTRINSIC: [[HEAD:v[0-9]+]] = sub {{v[0-9]+}}, 32
// INTRINSIC: mstore [[HEAD]], 32
// INTRINSIC: returndata [[HEAD]],
// PORTABLE-LABEL: fn @echo
// PORTABLE: tail_call @abiEncoded
import {Return} from "solar:core/v1/Return.sol";

contract Test {
    function echo(string memory s) public pure returns (string memory) {
        Return.abiEncoded(s);
    }

    // A value is one word, returned from scratch space.
    // INTRINSIC-LABEL: fn @word
    // INTRINSIC: mstore 0, arg0
    // INTRINSIC-NEXT: returndata 0, 32
    // PORTABLE-LABEL: fn @word
    // PORTABLE: tail_call @abiEncoded
    function word(uint256 value) public pure returns (uint256) {
        Return.abiEncoded(value);
    }

    // `Return.raw` returns the buffer's bytes as they are.
    // INTRINSIC-LABEL: fn @fallback
    // INTRINSIC: returndata {{v[0-9]+}}, {{v[0-9]+}}
    // PORTABLE-LABEL: fn @fallback
    // PORTABLE: tail_call @raw
    fallback() external {
        Return.raw(msg.data);
    }
}
