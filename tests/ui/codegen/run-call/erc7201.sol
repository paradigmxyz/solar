//@ run-call: literal() => 0x183a6125c38840424c4a85fa12bab2ab606c4b6d0e7cc73c0c06ba5300eab500
//@ run-call: zeroLastByteLiteral() => 0x6d0d983459328e82eacb1bf2d6fadfa38a6896e9d4cbfe0e1aa41c6281bab00
//@ run-call: memoryParam() => 0x183a6125c38840424c4a85fa12bab2ab606c4b6d0e7cc73c0c06ba5300eab500
// ported-from: test/cmdlineTests/yul_optimizer_erc7201_literal_comptime_evaluation/input.sol
// ported-from: test/cmdlineTests/yul_optimizer_erc7201_param_memory/input.sol

contract Erc7201RunCall {
    function literal() public pure returns (uint256) {
        return erc7201("example.main");
    }

    function zeroLastByteLiteral() public pure returns (uint256) {
        return erc7201("85");
    }

    function memoryParam() public pure returns (uint256) {
        string memory namespaceId = "example.main";
        return erc7201(namespaceId);
    }
}
