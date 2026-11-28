//@ codegen-matrix: standard
//@ compile-flags: --allow=0067
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: unaligned => 224, 2
//@ run-call: aligned => 192, 32

// Assembly that reads `msize` observes how far memory reaches. After an `abi.encodePacked` that
// does not end on a word, solc's final copy clears the word after the encoding, so memory reaches
// one word past the 64-byte object; a module that reads `msize` keeps that clear.
contract PackedMsize {
    function unaligned() external pure returns (uint256 m, uint256 length) {
        bytes memory b = abi.encodePacked(hex"0102");
        assembly {
            m := msize()
        }
        length = b.length;
    }

    function aligned() external pure returns (uint256 m, uint256 length) {
        bytes memory b = abi.encodePacked(bytes32(uint256(1)));
        assembly {
            m := msize()
        }
        length = b.length;
    }
}
