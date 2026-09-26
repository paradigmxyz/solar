// The fields of the builders in `solar:core/v1/Buffers.sol` belong to the
// module: a builder keeps what was written apart from its capacity, which a
// field read could expose and a field write could break.
import {Buffers, ByteBuilder, WordBuilder} from "solar:core/v1/Buffers.sol";

contract Test {
    using Buffers for ByteBuilder;
    using Buffers for WordBuilder;

    function peek(ByteBuilder memory b) internal pure returns (bytes memory) {
        return b.data; //~ ERROR: the fields of `ByteBuilder` belong to `Buffers`
    }

    function poke(ByteBuilder memory b) internal pure {
        b.used = 100; //~ ERROR: the fields of `ByteBuilder` belong to `Buffers`
    }

    function words(WordBuilder memory w) internal pure returns (uint256) {
        return w.used; //~ ERROR: the fields of `WordBuilder` belong to `Buffers`
    }

    function forge() internal pure returns (ByteBuilder memory) {
        return ByteBuilder(new bytes(10), 10); //~ ERROR: a `ByteBuilder` can only be made by `Buffers`
    }

    function named() internal pure returns (WordBuilder memory) {
        return WordBuilder({data: new uint256[](1), used: 1}); //~ ERROR: a `WordBuilder` can only be made by `Buffers`
    }

    // Functions attached with `using for` are not fields.
    function attached(ByteBuilder memory b) internal pure returns (uint256) {
        b.append("x");
        return b.length();
    }
}
