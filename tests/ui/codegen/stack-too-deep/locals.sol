//@ignore-host: windows
//@compile-flags: --emit=bin --pretty-json
// solc 0.8.30 without --via-ir reports `Stack too deep` for this contract.
pragma solidity ^0.8.0;

contract StackTooDeepLocals {
    function sum(uint256 x) external pure returns (uint256) {
        uint256 a0 = x + 0;
        uint256 a1 = x + 1;
        uint256 a2 = x + 2;
        uint256 a3 = x + 3;
        uint256 a4 = x + 4;
        uint256 a5 = x + 5;
        uint256 a6 = x + 6;
        uint256 a7 = x + 7;
        uint256 a8 = x + 8;
        uint256 a9 = x + 9;
        uint256 a10 = x + 10;
        uint256 a11 = x + 11;
        uint256 a12 = x + 12;
        uint256 a13 = x + 13;
        uint256 a14 = x + 14;
        uint256 a15 = x + 15;
        uint256 a16 = x + 16;
        uint256 a17 = x + 17;
        uint256 a18 = x + 18;
        uint256 a19 = x + 19;
        uint256 a20 = x + 20;
        uint256 a21 = x + 21;

        return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10
            + a11 + a12 + a13 + a14 + a15 + a16 + a17 + a18 + a19 + a20 + a21;
    }
}
