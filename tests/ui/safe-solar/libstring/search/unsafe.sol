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

// Solady's search, as shipped: a word compare of the needle's leading bytes
// at every position, one byte at a time, with a hash to confirm needles of a
// word or more. On every input above the two agree.
// CHECK-LABEL: fn @indexOf
// CHECK: keccak256
contract Unsafe {
    function indexOf(bytes memory subject, bytes memory needle, uint256 from)
        public
        pure
        returns (uint256 result)
    {
        assembly ("memory-safe") {
            result := not(0)
            for { let subjectLen := mload(subject) } 1 {} {
                if iszero(mload(needle)) {
                    result := from
                    if iszero(gt(from, subjectLen)) { break }
                    result := subjectLen
                    break
                }
                let needleLen := mload(needle)
                let subjectStart := add(subject, 0x20)
                subject := add(subjectStart, from)
                let end := add(sub(add(subjectStart, subjectLen), needleLen), 1)
                let m := shl(3, sub(0x20, and(needleLen, 0x1f)))
                let s := mload(add(needle, 0x20))
                if iszero(and(lt(subject, end), lt(from, subjectLen))) { break }
                if iszero(lt(needleLen, 0x20)) {
                    for { let h := keccak256(add(needle, 0x20), needleLen) } 1 {} {
                        if iszero(shr(m, xor(mload(subject), s))) {
                            if eq(keccak256(subject, needleLen), h) {
                                result := sub(subject, subjectStart)
                                break
                            }
                        }
                        subject := add(subject, 1)
                        if iszero(lt(subject, end)) { break }
                    }
                    break
                }
                for {} 1 {} {
                    if iszero(shr(m, xor(mload(subject), s))) {
                        result := sub(subject, subjectStart)
                        break
                    }
                    subject := add(subject, 1)
                    if iszero(lt(subject, end)) { break }
                }
                break
            }
        }
    }
}
