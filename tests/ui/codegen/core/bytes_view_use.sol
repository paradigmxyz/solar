//@ compile-flags: -Ogas --emit=bin

// A view can only be read in place: every use that could keep it or write
// through it is rejected, since it would tell the view from a copy.
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Test {
    using Bytes for bytes;

    function length(bytes memory x) internal pure returns (uint256) {
        return x.length;
    }

    function writeThrough(bytes memory b) public pure {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        v[0] = 0x01; //~ ERROR: the view `v` can only be read in place
    }

    function coreWriteThrough(bytes memory b) public pure {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        v.writeBytes1(0, 0x01); //~ ERROR: the view `v` can only be read in place
    }

    function share(bytes memory b) public pure returns (bytes memory w) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        w = v; //~ ERROR: the view `v` can only be read in place
    }

    function reassign(bytes memory b) public pure {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        v = b; //~ ERROR: the view `v` can only be read in place
    }

    function pass(bytes memory b) public pure returns (uint256) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        return length(v); //~ ERROR: the view `v` can only be read in place
    }

    function encode(bytes memory b) public pure returns (bytes memory) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        return abi.encode(v); //~ ERROR: the view `v` can only be read in place
    }

    function escape(bytes memory b) public pure returns (bytes memory) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        return v; //~ ERROR: the view `v` can only be read in place
    }
}
