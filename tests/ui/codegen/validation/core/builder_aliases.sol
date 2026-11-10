//@ revisions: bin abi
//@[bin] compile-flags: --emit=bin -Ogas
//@[abi] compile-flags: --emit=abi
import {Buffers, ByteBuilder} from "solar:core/Buffers.sol";

contract Test {
    function identity(ByteBuilder memory b) internal pure returns (ByteBuilder memory) { return b; } //~ ERROR: this uses a builder after `finish` emptied it

    function finishReturned() public pure returns (uint256) {
        ByteBuilder memory b = Buffers.create(4);
        Buffers.finish(identity(b));
        return Buffers.length(b); //~ ERROR: this uses a builder after `finish` emptied it
    }

    function useReturned() public pure returns (uint256) {
        ByteBuilder memory b = Buffers.create(4);
        ByteBuilder memory selectedBuilder = identity(b);
        Buffers.finish(b);
        return Buffers.length(selectedBuilder); //~ ERROR: this uses a builder after `finish` emptied it
    }

    function selectedBuilderedParameters() public pure {
        ByteBuilder memory b = Buffers.create(4);
        finishAndAppend(b, b);
    }

    function finishAndAppend(ByteBuilder memory a, ByteBuilder memory b) internal pure {
        Buffers.finish(a);
        Buffers.appendByte(b, 0x03); //~ ERROR: this uses a builder after `finish` emptied it
    }

    function selected(bool flag) public pure {
        ByteBuilder memory a = Buffers.create(4);
        ByteBuilder memory b = Buffers.create(4);
        ByteBuilder memory selectedBuilder = flag ? a : b;
        Buffers.finish(selectedBuilder);
        Buffers.appendByte(a, 0x03); //~ ERROR: this uses a builder after `finish` emptied it
    }
    function unusedCall() public pure {
        ByteBuilder memory b = Buffers.create(4);
        Buffers.finish(b);
        identity(b); //~ ERROR: this uses a builder after `finish` emptied it
    }

    function unusedReturn() public pure {
        emptyAndReturn(Buffers.create(4));
    }

    function emptyAndReturn(ByteBuilder memory b) internal pure returns (ByteBuilder memory) {
        Buffers.finish(b);
        return b; //~ ERROR: this uses a builder after `finish` emptied it
    }

}
