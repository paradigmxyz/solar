//@compile-flags: -Ztypeck
contract C {
    function f() public pure {
        // --- Valid: literal to fixed-size bytes (equal size) ---
        bytes3 b3_1 = "abc";
        bytes3 b3_2 = hex"123456";

        // --- Valid: literal to fixed-size bytes (larger size) ---
        bytes10 b10_1 = "abc";
        bytes10 b10_2 = hex"123456";

        // --- Invalid: literal to fixed-size bytes (smaller size) ---
        bytes2 invalid_b2 = "abc"; //~ ERROR: mismatched types
        bytes2 invalid_h2 = hex"123456"; //~ ERROR: mismatched types
        bytes1 invalid_b1 = "ab"; //~ ERROR: mismatched types
    }
}
