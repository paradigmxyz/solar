//@ revisions: bin abi
//@[bin] compile-flags: --emit=bin -Ogas
//@[abi] compile-flags: --emit=abi
import {Buffers, ByteBuilder} from "solar:core/Buffers.sol";

contract Test {
    function changedBound(bytes memory parts) public pure returns (bytes memory out) {
        ByteBuilder memory b = Buffers.create(parts.length);
        for (uint256 i; i < parts.length; ++i) {
            Buffers.appendByte(b, parts[i]); //~ ERROR: this uses a builder after `finish` emptied it
            if (i + 1 == parts.length) out = Buffers.finish(b); //~ ERROR: this uses a builder after `finish` emptied it
            // Even memory-safe assembly can change an existing object's header. Do not equate
            // subsequent length reads with the bound the finish condition saw.
            if (i == 2) assembly ("memory-safe") { mstore(parts, 4) }
        }
    }
}
