// ported-from: test/libsolidity/syntaxTests/array/length/too_large.sol
// ported-from: test/libsolidity/syntaxTests/array/length/uint_too_large_multidim.sol
// ported-from: test/libsolidity/syntaxTests/array/length/bytes32_too_large.sol
// ported-from: test/libsolidity/syntaxTests/largeTypes/oversized_array_1d.sol

contract TooLarge {
    uint[8**90] ids; //~ ERROR: array length is too large
    uint[2**256-1] okay;
    uint[2**256] tooLarge; //~ ERROR: array length is too large
}

contract UintTooLargeMultidim {
    uint[8**90][500] ids; //~ ERROR: array length is too large
}

contract Bytes32TooLarge {
    bytes32[8**90] ids; //~ ERROR: array length is too large
}

contract OversizedArray1d {
    uint[2**256] x; //~ ERROR: array length is too large
}
