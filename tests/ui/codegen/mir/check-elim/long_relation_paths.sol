//@ codegen-matrix: standard
//@ run-call: ordered [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65] => 1
//@ run-call-fail: ordered [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 0, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65]
//@ run-call: sibling true, 1, 2, 3 => 1
//@ run-call: sibling false, 3, 2, 1 => 0
//@ run-call: sibling false, 2, 1, 3 => 0
//@ run-call: sibling false, 1, 2, 3 => 1
//@ run-call: sibling false, 1, 3, 2 => 0
//@ run-call-fail: sibling true, 2, 1, 3
contract RelationPaths {
    function ordered(uint256[66] calldata a) external pure returns (uint256) {
        require(a[0] < a[1]);
        require(a[1] < a[2]);
        require(a[2] < a[3]);
        require(a[3] < a[4]);
        require(a[4] < a[5]);
        require(a[5] < a[6]);
        require(a[6] < a[7]);
        require(a[7] < a[8]);
        require(a[8] < a[9]);
        require(a[9] < a[10]);
        require(a[10] < a[11]);
        require(a[11] < a[12]);
        require(a[12] < a[13]);
        require(a[13] < a[14]);
        require(a[14] < a[15]);
        require(a[15] < a[16]);
        require(a[16] < a[17]);
        require(a[17] < a[18]);
        require(a[18] < a[19]);
        require(a[19] < a[20]);
        require(a[20] < a[21]);
        require(a[21] < a[22]);
        require(a[22] < a[23]);
        require(a[23] < a[24]);
        require(a[24] < a[25]);
        require(a[25] < a[26]);
        require(a[26] < a[27]);
        require(a[27] < a[28]);
        require(a[28] < a[29]);
        require(a[29] < a[30]);
        require(a[30] < a[31]);
        require(a[31] < a[32]);
        require(a[32] < a[33]);
        require(a[33] < a[34]);
        require(a[34] < a[35]);
        require(a[35] < a[36]);
        require(a[36] < a[37]);
        require(a[37] < a[38]);
        require(a[38] < a[39]);
        require(a[39] < a[40]);
        require(a[40] < a[41]);
        require(a[41] < a[42]);
        require(a[42] < a[43]);
        require(a[43] < a[44]);
        require(a[44] < a[45]);
        require(a[45] < a[46]);
        require(a[46] < a[47]);
        require(a[47] < a[48]);
        require(a[48] < a[49]);
        require(a[49] < a[50]);
        require(a[50] < a[51]);
        require(a[51] < a[52]);
        require(a[52] < a[53]);
        require(a[53] < a[54]);
        require(a[54] < a[55]);
        require(a[55] < a[56]);
        require(a[56] < a[57]);
        require(a[57] < a[58]);
        require(a[58] < a[59]);
        require(a[59] < a[60]);
        require(a[60] < a[61]);
        require(a[61] < a[62]);
        require(a[62] < a[63]);
        require(a[63] < a[64]);
        require(a[64] < a[65]);
        require(a[0] < a[65]);
        return 1;
    }

    function sibling(bool choice, uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        if (choice) {
            require(a < b);
            require(b < c);
            require(a < c);
        } else if (a >= c || b >= c) {
            return 0;
        }
        return a < b ? 1 : 0;
    }
}
