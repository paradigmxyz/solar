//@ compile-flags: -Ogas --emit=bin

// `Buffers.finish` returns what a builder holds and empties it, so a builder
// is finished once, after its last append: every use of a builder on a path
// after `finish` emptied it is rejected.
import {Buffers, ByteBuilder, WordBuilder} from "solar:core/v1/Buffers.sol";

contract Test {
    using Buffers for ByteBuilder;
    using Buffers for WordBuilder;

    function again() public pure returns (bytes memory out) {
        ByteBuilder memory b = Buffers.create(4);
        b.append("ab");
        out = b.finish();
        b.append("cd"); //~ ERROR: this uses a builder after `finish` emptied it
    }

    function twice() public pure returns (bytes memory, bytes memory) {
        ByteBuilder memory b = Buffers.create(4);
        bytes memory first = b.finish();
        return (first, b.finish()); //~ ERROR: this uses a builder after `finish` emptied it
    }

    // A copy of the reference is the same builder.
    function aliased() public pure returns (uint256) {
        ByteBuilder memory b = Buffers.create(4);
        ByteBuilder memory c = b;
        b.finish();
        return c.length(); //~ ERROR: this uses a builder after `finish` emptied it
    }

    // A later iteration appends to the builder an earlier one finished, and
    // may finish it again: paths are not told apart by their conditions.
    function loop(uint256 n) public pure returns (bytes memory out) {
        ByteBuilder memory b = Buffers.create(4);
        for (uint256 i; i < n; ++i) {
            b.appendByte(0x01); //~ ERROR: this uses a builder after `finish` emptied it
            if (i == 2) out = b.finish(); //~ ERROR: this uses a builder after `finish` emptied it
        }
    }

    // A helper that finishes its parameter finishes the caller's builder.
    function done(ByteBuilder memory b) internal pure returns (bytes memory) {
        b.appendByte(0x02);
        return b.finish();
    }

    function helper() public pure returns (bytes memory out, uint256 n) {
        ByteBuilder memory b = Buffers.create(4);
        out = done(b);
        n = b.length(); //~ ERROR: this uses a builder after `finish` emptied it
    }

    function words() public pure returns (uint256[] memory out) {
        WordBuilder memory w = Buffers.createWords(2);
        w.append(1);
        out = w.finish();
        w.append(2); //~ ERROR: this uses a builder after `finish` emptied it
    }

    // A new builder in the same variable starts over, and finishing on each
    // path at the end is fine.
    function restart(bool flag) public pure returns (bytes memory first, bytes memory second) {
        ByteBuilder memory b = Buffers.create(4);
        b.append("x");
        first = b.finish();
        b = Buffers.create(4);
        b.append("y");
        if (flag) {
            second = b.finish();
        } else {
            second = done(b);
        }
    }

    // Each iteration builds and finishes its own builder.
    function perIteration(uint256 n) public pure returns (bytes memory out) {
        for (uint256 i; i < n; ++i) {
            ByteBuilder memory b = Buffers.create(1);
            b.appendByte(bytes1(uint8(i)));
            out = b.finish();
        }
    }
}
