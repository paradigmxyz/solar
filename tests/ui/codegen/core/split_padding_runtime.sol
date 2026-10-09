//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: payloads "abc" => [0x6100000000000000000000000000000000000000000000000000000000000000, 0x6200000000000000000000000000000000000000000000000000000000000000, 0x6300000000000000000000000000000000000000000000000000000000000000]
//@ run-call: payloads "" => []
import {Strings} from "solar:core/Strings.sol";

// Splitting on an empty delimiter gives one-byte strings. Like the body's
// `new bytes(1)`, each payload word holds its byte and zero padding, not the
// subject bytes after it.
contract SplitPadding {
    function payloads(string memory subject) external pure returns (bytes32[] memory words) {
        string[] memory pieces = Strings.split(subject, "");
        words = new bytes32[](pieces.length);
        for (uint256 i; i < pieces.length; ++i) {
            string memory piece = pieces[i];
            bytes32 word;
            assembly {
                word := mload(add(piece, 0x20))
            }
            words[i] = word;
        }
    }
}
