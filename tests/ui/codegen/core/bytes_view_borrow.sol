//@ compile-flags: -Ogas --emit=bin

// A view reads its source's bytes in place, so a write that may change them
// between the view and a later read of it would make the view differ from the
// copy other compilers make. Writes after the last read, and writes of memory
// that cannot hold the source, are fine.
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Test {
    using Bytes for bytes;

    function overwrite(bytes memory b) public pure returns (bytes1) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        b[0] = 0x01; //~ ERROR: this may change bytes that the view `v` still reads
        return v[0];
    }

    function afterLastRead(bytes memory b) public pure returns (bytes1 first) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        first = v[0];
        b[0] = 0x01;
    }

    function throughAlias(bytes memory b) public pure returns (bytes1) {
        bytes memory c = b;
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        c[1] = 0x01; //~ ERROR: this may change bytes that the view `v` still reads
        return v[1];
    }

    // Another parameter may be the same object.
    function throughParameter(bytes memory b, bytes memory c) public pure returns (bytes1) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        c[0] = 0x01; //~ ERROR: this may change bytes that the view `v` still reads
        return v[0];
    }

    function clear(bytes memory x) internal pure {
        x[0] = 0;
    }

    function throughCall(bytes memory b) public pure returns (bytes1) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        clear(b); //~ ERROR: this may change bytes that the view `v` still reads
        return v[0];
    }

    function make(uint256 n) internal pure returns (bytes memory x) {
        x = new bytes(n);
        x[0] = 0x01;
    }

    // A callee that writes only memory it allocates cannot reach the source.
    function callWritingFreshMemory(bytes memory b) public pure returns (bytes1, bytes memory x) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        x = make(3);
        return (v[0], x);
    }

    // The next iteration reads the view after this one's write.
    function inLoop(bytes memory b) public pure returns (uint256 sum) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, b.length);
        for (uint256 i; i < v.length; ++i) {
            sum += uint8(v[i]);
            b[i] = 0; //~ ERROR: this may change bytes that the view `v` still reads
        }
    }

    function coreWrite(bytes memory b) public pure returns (bytes32) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 32);
        b.writeUint256BE(0, 1); //~ ERROR: this may change bytes that the view `v` still reads
        return keccak256(v);
    }

    // Assembly may write any word, but the scratch words below the heap hold
    // no object.
    function inAssembly(bytes memory b) public pure returns (uint8 first) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 1);
        assembly {
            mstore(0x00, 1)
        }
        first = uint8(v[0]);
        assembly {
            mstore(0x80, 1) //~ ERROR: this may change bytes that the view `v` still reads
        }
        first = uint8(v[0]);
    }

    // A raw store may run past a parameter's object into a fresh one.
    function pastParameter(bytes memory p) public pure returns (bytes1) {
        bytes memory data = new bytes(4);
        /// @custom:solar-view
        bytes memory v = data.slice(0, 2);
        assembly {
            mstore(add(p, 500), 1) //~ ERROR: this may change bytes that the view `v` still reads
        }
        return v[0];
    }
}
