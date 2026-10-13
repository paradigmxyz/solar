// Encoding or storing a builder would copy its capacity as well, bytes that
// were never written, so a builder stays in memory and out of the ABI.
import {Buffers, ByteBuilder, WordBuilder} from "solar:core/Buffers.sol";

contract Test {
    ByteBuilder stored; //~ ERROR: a `ByteBuilder` cannot be stored or encoded

    event Built(ByteBuilder b); //~ ERROR: a `ByteBuilder` cannot be stored or encoded

    error Unfinished(WordBuilder w); //~ ERROR: a `WordBuilder` cannot be stored or encoded

    function(ByteBuilder memory) external callback; //~ ERROR: a `ByteBuilder` cannot be stored or encoded

    constructor(ByteBuilder memory b) {} //~ ERROR: a `ByteBuilder` cannot be stored or encoded

    function take(ByteBuilder memory b) external pure {} //~ ERROR: a `ByteBuilder` cannot be stored or encoded

    function give() public pure returns (ByteBuilder memory b) { //~ ERROR: a `ByteBuilder` cannot be stored or encoded
        b = Buffers.create(1);
    }

    function encoded() internal pure returns (bytes memory) {
        ByteBuilder memory b = Buffers.create(64);
        return abi.encode(b); //~ ERROR: a `ByteBuilder` cannot be stored or encoded
    }

    function selected() internal pure returns (bytes memory) {
        WordBuilder memory w = Buffers.createWords(4);
        return abi.encodeWithSelector(0x12345678, 1, w); //~ ERROR: a `WordBuilder` cannot be stored or encoded
    }

    function signed() internal pure returns (bytes memory) {
        ByteBuilder memory b = Buffers.create(64);
        return abi.encodeWithSignature("f()", b); //~ ERROR: a `ByteBuilder` cannot be stored or encoded
    }

    function tried() internal view {
        try this.give() returns (ByteBuilder memory b) { //~ ERROR: a `ByteBuilder` cannot be stored or encoded
            b;
        } catch {}
    }

    // An external function type that takes or returns a builder encodes it when called,
    // wherever the type appears.
    function(ByteBuilder memory) external[] callbacks; //~ ERROR: a `ByteBuilder` cannot be stored or encoded

    mapping(uint256 => function(WordBuilder memory) external) byKey; //~ ERROR: a `WordBuilder` cannot be stored or encoded

    function nested(function(function(ByteBuilder memory) external) external f) internal {} //~ ERROR: a `ByteBuilder` cannot be stored or encoded

    function freshCallback() internal {
        ByteBuilder memory b = Buffers.create(64);
        new function(ByteBuilder memory) external[](1)[0](b); //~ ERROR: a `ByteBuilder` cannot be stored or encoded
    }

    // Memory locals and internal parameters are fine, and so is what
    // `finish` returns.
    function kept(ByteBuilder memory b) internal pure returns (ByteBuilder memory) {
        return b;
    }

    function finished() internal pure returns (bytes memory) {
        ByteBuilder memory b = Buffers.create(64);
        Buffers.append(b, "x");
        return abi.encode(Buffers.finish(kept(b)));
    }
}
