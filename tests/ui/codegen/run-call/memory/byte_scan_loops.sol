//@ codegen-matrix: standard
//@ run-call: ascii 0x => true
//@ run-call: ascii 0x000102037f => true
//@ run-call: ascii 0x0001020380 => false
//@ run-call: ascii 0x6161616161616161616161616161616161616161616161616161616161616161 => true
//@ run-call: ascii 0x6161616161616161616161616161616161616161616161616161616161616180 => false
//@ run-call: zeroBytes 0x => 0
//@ run-call: zeroBytes 0x00 => 1
//@ run-call: zeroBytes 0x00010002 => 2
//@ run-call: zeroBytes 0x0101010101010101010101010101010101010101010101010101010101010101 => 0
//@ run-call: zeroBytes 0x000000000000000000000000000000000000000000000000000000000000000001 => 32
//@ run-call: zeroCalldata 0x => 0
//@ run-call: zeroCalldata 0x00 => 1
//@ run-call: zeroCalldata 0x00010002 => 2
//@ run-call: zeroCalldata 0x0101010101010101010101010101010101010101010101010101010101010101 => 0
//@ run-call: zeroCalldata 0x000000000000000000000000000000000000000000000000000000000000000001 => 32
contract Test {
    function ascii(bytes memory s) public pure returns (bool) {
        for (uint256 i; i < s.length; ++i) {
            if (s[i] > 0x7f) return false;
        }
        return true;
    }

    function zeroBytes(bytes memory s) public pure returns (uint256 count) {
        for (uint256 i; i < s.length; ++i) {
            if (s[i] == 0) ++count;
        }
    }

    function zeroCalldata(bytes calldata s) public pure returns (uint256 count) {
        for (uint256 i; i < s.length; ++i) {
            if (s[i] == 0) ++count;
        }
    }
}
