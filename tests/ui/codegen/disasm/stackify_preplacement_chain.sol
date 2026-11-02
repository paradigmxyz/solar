//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: f 1, 2 => 3
//@ run-call: f 5, 6 => 4

// Each `round` returns two values that the next call takes, so the single-use producers of a
// call's operand form a chain in which every call is reached through each of its two results.
// Finding where the chain feeding a consumer starts must visit each instruction once and stop
// past the preplacement window; walking every path doubles compile time with each call.
contract StackifyPreplacementChain {
    function f(uint256 s0, uint256 s1) external pure returns (uint256) {
        (s0, s1) = round(s0, s1, 3);
        (s0, s1) = round(s0, s1, 4);
        (s0, s1) = round(s0, s1, 5);
        (s0, s1) = round(s0, s1, 6);
        (s0, s1) = round(s0, s1, 7);
        (s0, s1) = round(s0, s1, 8);
        (s0, s1) = round(s0, s1, 9);
        (s0, s1) = round(s0, s1, 10);
        (s0, s1) = round(s0, s1, 11);
        (s0, s1) = round(s0, s1, 12);
        (s0, s1) = round(s0, s1, 13);
        (s0, s1) = round(s0, s1, 14);
        (s0, s1) = round(s0, s1, 15);
        (s0, s1) = round(s0, s1, 16);
        (s0, s1) = round(s0, s1, 17);
        (s0, s1) = round(s0, s1, 18);
        (s0, s1) = round(s0, s1, 19);
        (s0, s1) = round(s0, s1, 20);
        (s0, s1) = round(s0, s1, 21);
        (s0, s1) = round(s0, s1, 22);
        (s0, s1) = round(s0, s1, 23);
        (s0, s1) = round(s0, s1, 24);
        (s0, s1) = round(s0, s1, 25);
        (s0, s1) = round(s0, s1, 26);
        (s0, s1) = round(s0, s1, 27);
        (s0, s1) = round(s0, s1, 28);
        (s0, s1) = round(s0, s1, 29);
        (s0, s1) = round(s0, s1, 30);
        (s0, s1) = round(s0, s1, 31);
        (s0, s1) = round(s0, s1, 32);
        (s0, s1) = round(s0, s1, 33);
        (s0, s1) = round(s0, s1, 34);
        (s0, s1) = round(s0, s1, 35);
        (s0, s1) = round(s0, s1, 36);
        (s0, s1) = round(s0, s1, 37);
        (s0, s1) = round(s0, s1, 38);
        (s0, s1) = round(s0, s1, 39);
        (s0, s1) = round(s0, s1, 40);
        (s0, s1) = round(s0, s1, 41);
        (s0, s1) = round(s0, s1, 42);
        (s0, s1) = round(s0, s1, 43);
        (s0, s1) = round(s0, s1, 44);
        (s0, s1) = round(s0, s1, 45);
        (s0, s1) = round(s0, s1, 46);
        (s0, s1) = round(s0, s1, 47);
        (s0, s1) = round(s0, s1, 48);
        (s0, s1) = round(s0, s1, 49);
        (s0, s1) = round(s0, s1, 50);
        (s0, s1) = round(s0, s1, 51);
        (s0, s1) = round(s0, s1, 52);
        (s0, s1) = round(s0, s1, 53);
        (s0, s1) = round(s0, s1, 54);
        (s0, s1) = round(s0, s1, 55);
        (s0, s1) = round(s0, s1, 56);
        (s0, s1) = round(s0, s1, 57);
        (s0, s1) = round(s0, s1, 58);
        (s0, s1) = round(s0, s1, 59);
        (s0, s1) = round(s0, s1, 60);
        (s0, s1) = round(s0, s1, 61);
        (s0, s1) = round(s0, s1, 62);
        (s0, s1) = round(s0, s1, 63);
        (s0, s1) = round(s0, s1, 64);
        (s0, s1) = round(s0, s1, 65);
        (s0, s1) = round(s0, s1, 66);
        return s0 ^ s1;
    }

    function round(uint256 s0, uint256 s1, uint256 k) private pure returns (uint256, uint256) {
        uint256 x = addmod(s0, k, 7);
        x = mulmod(x, mulmod(x, x, 7), 7);
        uint256 sum = addmod(x, s1, 7);
        return (addmod(x, sum, 7), addmod(s1, sum, 7));
    }
}
