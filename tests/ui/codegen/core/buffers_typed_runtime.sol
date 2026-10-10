//@ codegen-matrix: standard
//@ run-call: addresses 0 => [], 0
//@ run-call: addresses 5 => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000003, 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000005], 5
//@ run-call: words 3 => [0x0000000000000000000000000000000000000000000000000000000000000000, 0x0101010101010101010101010101010101010101010101010101010101010101, 0x0202020202020202020202020202020202020202020202020202020202020202], 3
//@ run-call: signed 4 => [0, -1, -2, -3], 4
//@ run-call: neighbour 7 => [7, 8, 9, 10, 11, 12, 13], 0xdeadbeef

// The typed builders of `Buffers` assemble an `address[]`, a `bytes32[]` and
// an `int256[]` as a `WordBuilder` assembles a `uint256[]`: appends grow the
// builder instead of writing past it, and `finish` hands the words over
// without a copy.
import {AddressBuilder, Buffers, Bytes32Builder, Int256Builder} from "solar:core/Buffers.sol";

contract Test {
    using Buffers for AddressBuilder;
    using Buffers for Bytes32Builder;
    using Buffers for Int256Builder;

    function addresses(uint256 n) public pure returns (address[] memory out, uint256 length) {
        AddressBuilder memory builder = Buffers.createAddresses(1);
        for (uint256 i = 1; i <= n; ++i) {
            builder.append(address(uint160(i)));
        }
        length = builder.length();
        out = builder.finish();
    }

    function words(uint256 n) public pure returns (bytes32[] memory out, uint256 length) {
        Bytes32Builder memory builder = Buffers.createBytes32s(0);
        for (uint256 i; i < n; ++i) {
            builder.append(bytes32(i * 0x0101010101010101010101010101010101010101010101010101010101010101));
        }
        length = builder.length();
        out = builder.finish();
    }

    function signed(uint256 n) public pure returns (int256[] memory out, uint256 length) {
        Int256Builder memory builder = Buffers.createInt256s(2);
        for (uint256 i; i < n; ++i) {
            builder.append(-int256(i));
        }
        length = builder.length();
        out = builder.finish();
    }

    // Growing moves the words, and the allocation that follows keeps its own.
    function neighbour(uint256 start) public pure returns (int256[] memory out, bytes4 kept) {
        Int256Builder memory builder = Buffers.createInt256s(1);
        bytes memory after_ = hex"deadbeef";
        for (uint256 i; i < 7; ++i) {
            builder.append(int256(start + i));
        }
        out = builder.finish();
        kept = bytes4(after_);
    }
}
