//@ revisions: intrinsic portable size
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[size] compile-flags: -Osize
//@ run-call: indexOf "abcb", "b" => 1
//@ run-call: attached "abcb", "b" => 3
//@ run-call: read 0x0011223344 => 0x11223344
//@ run-call: ordered => 0x11223344, 12
//@ run-call: encodingFirst 7 => 0x0000000000000000000000000000000000000000000000000000000000000007, 32
import {Abi} from "solar:core/Abi.sol";
import {Bytes} from "solar:core/Bytes.sol";
import {Strings} from "solar:core/Strings.sol";

// Named arguments bind by name, not by their order in the call, and evaluate
// in source order, for the compiler-owned modules as for any other function.
contract NamedArguments {
    using Strings for string;

    uint256 trace;

    function indexOf(string memory subject, string memory needle) external pure returns (uint256) {
        return Strings.indexOf({needle: needle, subject: subject, from: 0});
    }

    function attached(string memory subject, string memory needle) external pure returns (uint256) {
        return subject.indexOf({from: 2, needle: needle});
    }

    function read(bytes memory b) external pure returns (bytes4) {
        return Bytes.readBytes4({offset: 1, b: b});
    }

    function tagged(uint256 tag) internal returns (uint256) {
        trace = trace * 10 + tag;
        return 1;
    }

    function taggedBytes(uint256 tag, bytes memory b) internal returns (bytes memory) {
        trace = trace * 10 + tag;
        return b;
    }

    function ordered() external returns (bytes4 word, uint256 order) {
        word = Bytes.readBytes4({offset: tagged(1), b: taggedBytes(2, hex"0011223344")});
        order = trace;
    }

    // The encoding comes first and the buffer after it allocates, so the
    // encoding cannot wait past the free memory pointer for the copy.
    function encodingFirst(uint256 value) external pure returns (bytes memory out, uint256 written) {
        written = Abi.writeEncoding({encoding: abi.encode(value), offset: 0, out: out = new bytes(32)});
    }
}
