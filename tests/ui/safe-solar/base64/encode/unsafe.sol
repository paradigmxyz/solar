//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: encode 0x, false, false => 0x
//@ run-call: encode 0x0b, false, false => 0x43773d3d
//@ run-call: encode 0x0b, false, true => 0x4377
//@ run-call: encode 0x0b3055, false, false => 0x437a4256
//@ run-call: encode 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef1439, true, false => 0x437a425665705f453651347a57483269782d77524e6c75417063727646446b3d
//@ run-call: encode 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e, false, false => 0x437a425665702f453651347a57483269782b77524e6c75417063727646446c65
//@ run-call: encode 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83, false, true => 0x437a425665702f453651347a57483269782b77524e6c75417063727646446c656777
//@ run-call: encode 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6, true, false => 0x437a425665705f453651347a57483269782d77524e6c75417063727646446c6567366a4e3868633859596172305055615032534a7274503448554a6e6a4c4857

// Solady's encoder, as shipped: the alphabet parked in scratch space, three
// input bytes read as a word, four characters looked up with four byte
// stores each. On every input above the two agree byte for byte.
// CHECK-LABEL: fn @encode
// CHECK: mstore8
contract Unsafe {
    function encode(bytes memory data, bool fileSafe, bool noPadding)
        public
        pure
        returns (bytes memory result)
    {
        assembly ("memory-safe") {
            let dataLength := mload(data)
            if dataLength {
                let encodedLength := shl(2, div(add(dataLength, 2), 3))
                result := mload(0x40)
                mstore(0x1f, "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef")
                mstore(0x3f, xor("ghijklmnopqrstuvwxyz0123456789-_", mul(iszero(fileSafe), 0x0670)))
                let ptr := add(result, 0x20)
                let end := add(ptr, encodedLength)
                let dataEnd := add(add(0x20, data), dataLength)
                let dataEndValue := mload(dataEnd)
                mstore(dataEnd, 0x00)
                for {} 1 {} {
                    data := add(data, 3)
                    let input := mload(data)
                    mstore8(0, mload(and(shr(18, input), 0x3F)))
                    mstore8(1, mload(and(shr(12, input), 0x3F)))
                    mstore8(2, mload(and(shr(6, input), 0x3F)))
                    mstore8(3, mload(and(input, 0x3F)))
                    mstore(ptr, mload(0x00))
                    ptr := add(ptr, 4)
                    if iszero(lt(ptr, end)) { break }
                }
                mstore(dataEnd, dataEndValue)
                mstore(0x40, add(end, 0x20))
                let o := div(2, mod(dataLength, 3))
                mstore(sub(ptr, o), shl(240, 0x3d3d))
                o := mul(iszero(iszero(noPadding)), o)
                mstore(sub(ptr, o), 0)
                mstore(result, sub(encodedLength, o))
            }
        }
    }
}
