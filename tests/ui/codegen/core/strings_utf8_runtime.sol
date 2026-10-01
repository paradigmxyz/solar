//@ revisions: gas size none
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[none] compile-flags: -Onone
//@ run-call: valid 0x => true
//@ run-call: valid 0x616263 => true
//@ run-call: valid 0xc2a9e282acf09d849e => true
//@ run-call: valid 0xdfbfe0a080ed9fbfee8080f0908080f48fbfbf => true
//@ run-call: valid 0x78787878787878787878787878787878787878787878787878787878787878787878c3a9 => true
//@ run-call: valid 0xc080 => false
//@ run-call: valid 0xc1bf => false
//@ run-call: valid 0xe08080 => false
//@ run-call: valid 0xeda080 => false
//@ run-call: valid 0xf0808080 => false
//@ run-call: valid 0xf4908080 => false
//@ run-call: valid 0xf5808080 => false
//@ run-call: valid 0x80 => false
//@ run-call: valid 0xe282 => false
//@ run-call: valid 0xc378 => false
//@ run-call: valid 0x787878787878787878787878787878787878787878787878787878787878787878ff => false

// `Strings.isValidUTF8` accepts exactly the well-formed UTF-8 of RFC 3629:
// shortest forms only, no surrogate halves, nothing past U+10FFFF, and no
// truncated rune or stray continuation byte. Runs of ASCII are skipped a word
// at a time, so the last two cases put a rune after a full ASCII word.
import {Strings} from "solar:core/Strings.sol";

contract Test {
    function valid(bytes memory b) public pure returns (bool) {
        return Strings.isValidUTF8(string(b));
    }
}
