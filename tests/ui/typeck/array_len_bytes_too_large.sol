// `bytes` and `string` values larger than 64 KiB do not fold, so doubling concatenations stay
// cheap to compile.
bytes constant B0 = "0123456789abcdef0123456789abcdef";
bytes constant B1 = bytes.concat(B0, B0);
bytes constant B2 = bytes.concat(B1, B1);
bytes constant B3 = bytes.concat(B2, B2);
bytes constant B4 = bytes.concat(B3, B3);
bytes constant B5 = bytes.concat(B4, B4);
bytes constant B6 = bytes.concat(B5, B5);
bytes constant B7 = bytes.concat(B6, B6);
bytes constant B8 = bytes.concat(B7, B7);
bytes constant B9 = bytes.concat(B8, B8);
bytes constant B10 = bytes.concat(B9, B9);
bytes constant B11 = bytes.concat(B10, B10);
bytes constant B12 = bytes.concat(B11, B11);
string constant S = string.concat(string(B11), string(B11), "x");

contract C {
    uint[B11.length] fits;
    uint[B12.length] tooLarge; //~ ERROR: failed to evaluate constant: byte value is too large
    uint[bytes(S).length] tooLargeString; //~ ERROR: failed to evaluate constant: byte value is too large
}
