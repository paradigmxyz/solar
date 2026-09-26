//@ codegen-matrix: standard
//@ run-call: call 0x000000000000000000000000000000000000dEaD, 1 => 0xa9059cbb000000000000000000000000000000000000000000000000000000000000dead0000000000000000000000000000000000000000000000000000000000000001
//@ run-call: call 0x0000000000000000000000000000000000000000, 0 => 0xa9059cbb00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
//@ run-call: pair -1, true => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0000000000000000000000000000000000000000000000000000000000000001
//@ run-call: pair 0, false => 0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
//@ run-call: keepsSurroundings => 0xaaaa000000000000000000000000000000000000000000000000000000000000002a2222
//@ run-call: fitsReport 0 => true, 32
//@ run-call: fitsReport 32 => true, 32
//@ run-call: fitsReport 33 => false, 0
//@ run-call: fitsReport 115792089237316195423570985008687907853269984665640564039457584007913129639935 => false, 0
//@ run-call-fail: overflows => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// Encoding into memory the caller owns. A word write pads a narrow value, a
// selector write takes four bytes, every write is bounds checked against the
// buffer, and bytes outside the encoded range keep whatever they held.
import {Abi} from "solar:core/v1/Abi.sol";

contract Test {
    /// A call payload built in place: selector, then one word per argument.
    function call(address to, uint256 amount) public pure returns (bytes memory payload) {
        payload = new bytes(Abi.SELECTOR + 2 * Abi.WORD);
        uint256 at = Abi.encodeSelectorInto(payload, 0, bytes4(0xa9059cbb));
        at += Abi.encodeInto(payload, at, to);
        at += Abi.encodeInto(payload, at, amount);
        require(at == payload.length);
    }

    /// A signed word keeps its sign extension; a bool is one or zero.
    function pair(int256 signed, bool flag) public pure returns (bytes memory out) {
        out = new bytes(2 * Abi.WORD);
        uint256 at = Abi.encodeInto(out, 0, signed);
        Abi.encodeInto(out, at, flag);
    }

    /// The two bytes before the word and the two after it are untouched.
    function keepsSurroundings() public pure returns (bytes memory out) {
        out = new bytes(2 + Abi.WORD + 2);
        out[0] = 0xaa;
        out[1] = 0xaa;
        out[out.length - 2] = 0x22;
        out[out.length - 1] = 0x22;
        Abi.encodeInto(out, 2, uint256(42));
    }

    /// A write that does not fit reports instead of failing.
    function fitsReport(uint256 offset) public pure returns (bool ok, uint256 written) {
        bytes memory out = new bytes(2 * Abi.WORD);
        return Abi.tryEncodeInto(out, offset, uint256(1));
    }

    /// An offset near the top of the word cannot wrap into range.
    function overflows() public pure returns (bytes memory out) {
        out = new bytes(Abi.WORD);
        Abi.encodeInto(out, type(uint256).max - 8, uint256(1));
    }
}
