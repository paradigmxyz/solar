//@ codegen-matrix: standard
//@ compile-flags: --no-warnings
//@ run-call: packed() => 77, 77
//@ run-call: concatBytes() => 77, 77
//@ run-call: concatString() => 77, 77

// The packed result stores its length word before it copies the source, so a source whose
// assembly-forged length reaches past the free memory pointer copies that word into the result
// (CODEGEN-032). solc copies first and stores the length last, so byte 63 is 0 there.
contract C {
    function source() internal pure returns (bytes memory s) {
        assembly {
            s := mload(0x40)
            mstore(s, 77)
            mstore(add(s, 0x20), 0)
            mstore(0x40, add(s, 0x40))
        }
    }

    function packed() external pure returns (uint256, uint256) {
        bytes memory r = abi.encodePacked(source());
        return (r.length, uint8(r[63]));
    }

    function concatBytes() external pure returns (uint256, uint256) {
        bytes memory r = bytes.concat(source());
        return (r.length, uint8(r[63]));
    }

    function concatString() external pure returns (uint256, uint256) {
        bytes memory s = source();
        string memory t;
        assembly {
            t := s
        }
        bytes memory r = bytes(string.concat(t));
        return (r.length, uint8(r[63]));
    }
}
