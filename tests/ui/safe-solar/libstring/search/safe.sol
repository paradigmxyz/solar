//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: indexOf 0x74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f673b2074686520656e64206f6620746865207375626a656374206c696e6521, 0x746865, 0 => 0
//@ run-call: indexOf 0x74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f673b2074686520656e64206f6620746865207375626a656374206c696e6521, 0x746865, 1 => 31
//@ run-call: indexOf 0x74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f673b2074686520656e64206f6620746865207375626a656374206c696e6521, 0x746865, 40 => 45
//@ run-call: indexOf 0x74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f673b2074686520656e64206f6620746865207375626a656374206c696e6521, 0x6c696e6521, 0 => 68
//@ run-call: indexOf 0x74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f673b2074686520656e64206f6620746865207375626a656374206c696e6521, 0x65, 70 => 71
//@ run-call: indexOf 0x74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f673b2074686520656e64206f6620746865207375626a656374206c696e6521, 0x71, 0 => 4
//@ run-call: indexOf 0x74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f673b2074686520656e64206f6620746865207375626a656374206c696e6521, 0x666f78206a756d7073206f76657220746865206c617a79, 0 => 16
//@ run-call: indexOf 0x74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f673b2074686520656e64206f6620746865207375626a656374206c696e6521, 0x636174, 0 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: indexOf 0x74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f673b2074686520656e64206f6620746865207375626a656374206c696e6521, 0x, 5 => 5
//@ run-call: indexOf 0x74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f673b2074686520656e64206f6620746865207375626a656374206c696e6521, 0x, 99 => 73
//@ run-call: indexOf 0x74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f673b2074686520656e64206f6620746865207375626a656374206c696e6521, 0x746865, 73 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: indexOf 0x616263616263616264, 0x616264, 0 => 6
//@ run-call: indexOf 0x616263616263616264, 0x7a, 0 => 115792089237316195423570985008687907853269984665640564039457584007913129639935

// Substring search in checked Solidity: thirty-two candidate positions are
// rejected at a time. The word holding each position's first byte is
// exclusive-ored against the needle's first byte broadcast to a word, and a
// zero byte marks agreement; the second byte is tested the same way, so only
// positions where both agree pay for a full comparison. The last block is
// pulled back to stay inside the subject and the positions it covers twice
// are masked off.
// CHECK-LABEL: fn @_scanWords
// CHECK: mload
// CHECK: 0x7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f
// CHECK-NOT: icall @readBytes32
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Safe {
    uint256 private constant NOT_FOUND = type(uint256).max;
    uint256 private constant ONES =
        0x0101010101010101010101010101010101010101010101010101010101010101;
    uint256 private constant LANES_7F =
        0x7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f;

    function indexOf(bytes memory s, bytes memory n, uint256 from) public pure returns (uint256) {
        if (n.length == 0) return from > s.length ? s.length : from;
        if (n.length > s.length || from > s.length - n.length) return NOT_FOUND;
        uint256 last = s.length - n.length;
        if (s.length < 32) return _scan(s, n, from, last);
        return _scanWords(s, n, from, last);
    }

    function _scan(bytes memory s, bytes memory n, uint256 from, uint256 last)
        private
        pure
        returns (uint256)
    {
        for (uint256 i = from; i <= last; ++i) {
            if (s[i] == n[0] && _matchAt(s, n, i)) return i;
        }
        return NOT_FOUND;
    }

    function _scanWords(bytes memory s, bytes memory n, uint256 from, uint256 last)
        private
        pure
        returns (uint256)
    {
        uint256 first = uint256(uint8(n[0])) * ONES;
        uint256 second = n.length > 1 ? uint256(uint8(n[1])) * ONES : 0;
        uint256 i = from;
        while (i <= last) {
            uint256 at = i + 32 > s.length ? s.length - 32 : i;
            uint256 word = uint256(Bytes.readBytes32(s, at));
            uint256 found = _zeroBytes(word ^ first);
            if (at < i) found &= type(uint256).max >> ((i - at) * 8);
            if (last - at < 31) found &= ~(type(uint256).max >> ((last - at + 1) * 8));
            if (n.length > 1 && found != 0) {
                uint256 next = at + 32 < s.length ? uint256(uint8(s[at + 32])) : 0;
                found &= _zeroBytes(((word << 8) | next) ^ second);
            }
            while (found != 0) {
                uint256 hit = at + _firstMarkedByte(found);
                if (n.length < 3 || _matchAt(s, n, hit)) return hit;
                found &= ~(uint256(0xff) << (248 - (hit - at) * 8));
            }
            i = at + 32;
        }
        return NOT_FOUND;
    }

    function _matchAt(bytes memory s, bytes memory n, uint256 i) private pure returns (bool) {
        for (uint256 k; k < n.length; ++k) {
            if (s[i + k] != n[k]) return false;
        }
        return true;
    }

    function _zeroBytes(uint256 word) private pure returns (uint256) {
        return ~(word | ((word & LANES_7F) + LANES_7F) | LANES_7F);
    }

    function _firstMarkedByte(uint256 marks) private pure returns (uint256 index) {
        if (marks >> 128 == 0) {
            index = 16;
            marks <<= 128;
        }
        if (marks >> 192 == 0) {
            index += 8;
            marks <<= 64;
        }
        if (marks >> 224 == 0) {
            index += 4;
            marks <<= 32;
        }
        if (marks >> 240 == 0) {
            index += 2;
            marks <<= 16;
        }
        if (marks >> 248 == 0) index += 1;
    }
}
