//@ compile-flags: -Ogas --emit=bin

// A view that a decode makes of memory borrows the decoded bytes like any view:
// a write that may change them before a later read of the view is rejected. A
// view of calldata borrows nothing, since calldata cannot change, and hashing
// or comparing one writes only memory no object holds.
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Test {
    using Bytes for bytes;

    function length(bytes memory x) internal pure returns (uint256) {
        return x.length;
    }

    function overwrite(bytes memory data) public pure returns (bytes1) {
        /// @custom:solar-view
        (uint256 at, bytes memory v) = abi.decode(data, (uint256, bytes));
        data[at] = 0x01; //~ ERROR: this may change bytes that the view `v` still reads
        return v[0];
    }

    // A decode of a view borrows the view's source.
    function throughView(bytes memory packet) public pure returns (bytes1) {
        /// @custom:solar-view
        bytes memory body = packet.slice(4, packet.length - 4);
        /// @custom:solar-view
        (bytes memory v) = abi.decode(body, (bytes));
        packet.writeBytes1(100, 0x01); //~ ERROR: this may change bytes that the view `v` still reads
        return v[0];
    }

    function afterLastRead(bytes memory data) public pure returns (bytes1 first) {
        /// @custom:solar-view
        (bytes memory v) = abi.decode(data, (bytes));
        first = v[0];
        data[0] = 0x01;
    }

    // An untagged decode copies, so later writes of the data are fine.
    function copied(bytes memory data) public pure returns (bytes1) {
        (bytes memory v) = abi.decode(data, (bytes));
        data[0] = 0x01;
        return v[0];
    }

    function calldataView(bytes calldata data, bytes memory other) external pure returns (bytes1) {
        /// @custom:solar-view
        (bytes memory v) = abi.decode(data, (bytes));
        other[0] = 0x01;
        return v[0];
    }

    function hashNextToMemoryView(bytes memory m, bytes calldata c)
        external
        pure
        returns (bytes32 hash, bool same, bytes1 first)
    {
        /// @custom:solar-view
        (bytes memory mv) = abi.decode(m, (bytes));
        /// @custom:solar-view
        (bytes memory cv) = abi.decode(c, (bytes));
        hash = keccak256(cv);
        same = cv.equalsAt(0, mv);
        first = mv[0];
    }

    function passView(bytes calldata data) external pure returns (uint256) {
        /// @custom:solar-view
        (bytes memory v) = abi.decode(data, (bytes));
        return length(v); //~ ERROR: the view `v` can only be read in place
    }

    function returnView(bytes memory data) public pure returns (string memory) {
        /// @custom:solar-view
        (uint256 n, string memory s) = abi.decode(data, (uint256, string));
        return n == 0 ? "" : s; //~ ERROR: the view `s` can only be read in place
    }

    function writeView(bytes calldata data) external pure {
        /// @custom:solar-view
        (bytes memory v) = abi.decode(data, (bytes));
        v[0] = 0x01; //~ ERROR: the view `v` can only be read in place
    }
}
