//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: payloads "abc" => [0x6100000000000000000000000000000000000000000000000000000000000000, 0x6200000000000000000000000000000000000000000000000000000000000000, 0x6300000000000000000000000000000000000000000000000000000000000000]
//@ run-call: payloads "" => []
//@ run-call: dirtyPayloads "a,b", "," => [0x6100000000000000000000000000000000000000000000000000000000000000, 0x6200000000000000000000000000000000000000000000000000000000000000]
//@ run-call: dirtyReplaced "abc", "b", "x" => 0x6178630000000000000000000000000000000000000000000000000000000000
import {Strings} from "solar:core/Strings.sol";

// Splitting on an empty delimiter gives one-byte strings. Like the body's
// `new bytes(1)`, each payload word holds its byte and zero padding, not the
// subject bytes after it.
// The same holds when the memory the pieces and a replacement's output land
// in was dirty.
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

    function dirtyPayloads(string memory subject, string memory delimiter)
        external
        pure
        returns (bytes32[] memory words)
    {
        dirtyFreeMemory();
        string[] memory pieces = Strings.split(subject, delimiter);
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

    function dirtyReplaced(string memory subject, string memory needle, string memory replacement)
        external
        pure
        returns (bytes32 word)
    {
        dirtyFreeMemory();
        string memory replaced = Strings.replace(subject, needle, replacement);
        assembly {
            word := mload(add(replaced, 0x20))
        }
    }

    function dirtyFreeMemory() internal pure {
        assembly {
            let free := mload(0x40)
            for { let i := 0 } lt(i, 0x200) { i := add(i, 0x20) } {
                mstore(add(free, i), not(0))
            }
        }
    }
}
