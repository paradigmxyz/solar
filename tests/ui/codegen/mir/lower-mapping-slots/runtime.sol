//@ codegen-matrix: standard
//@ run-call: words 8 => 56
//@ run-call: bits 255, 127 => 128

contract SlotHashRuntime {
    mapping(uint256 => uint256) table;

    function words(uint256 n) external returns (uint256 sum) {
        table[17] = 7;
        for (uint256 i; i < n; ++i) sum += table[17];
    }

    function bits(uint8 a, uint8 b) external pure returns (uint256) {
        return uint256(a ^ b);
    }
}
