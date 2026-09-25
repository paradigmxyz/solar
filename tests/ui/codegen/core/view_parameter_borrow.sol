//@ compile-flags: -Ogas --emit=bin

// A view parameter reads the caller's bytes in place, so the function may only
// read it in place, and nothing the function does may change those bytes
// while it still reads them: writes of the memory that existed when it was
// entered are rejected, writes of fresh memory are not.
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Test {
    /// @custom:solar-view data
    function write(bytes memory data) internal pure {
        data[0] = 0x01; //~ ERROR: the view `data` can only be read in place
    }

    /// @custom:solar-view data
    function keep(bytes memory data) internal pure returns (bytes memory) {
        return data; //~ ERROR: the view `data` can only be read in place
    }

    // Another parameter may be the same object as the caller's bytes.
    /// @custom:solar-view data
    function throughOther(bytes memory data, bytes memory other) internal pure returns (bytes1) {
        other[0] = 0x01; //~ ERROR: this may change bytes that the view `data` still reads
        return data[0];
    }

    /// @custom:solar-view data
    function fresh(bytes memory data) internal pure returns (bytes1 first, bytes memory out) {
        out = new bytes(2);
        out[0] = 0x01;
        first = data[0];
    }

    /// @custom:solar-view data
    function afterLastRead(bytes memory data, bytes memory other) internal pure returns (bytes1 first) {
        first = data[0];
        other[0] = 0x01;
    }

    function pointer() internal pure returns (uint256) {
        function(bytes memory) internal pure f = write; //~ ERROR: a function with `@custom:solar-view` parameters cannot be used as a value
        f("x");
        return 0;
    }

    function run(bytes memory b, bytes memory c) public pure returns (bytes1, bytes1, uint256) {
        write(b);
        keep(b);
        (bytes1 first, ) = fresh(b);
        return (throughOther(b, c), first ^ afterLastRead(b, c), pointer());
    }
}
