//@compile-flags: -Zcodegen --emit=mir
// ported-from: test/cmdlineTests/yul_optimizer_erc7201_literal_comptime_evaluation/input.sol
// ported-from: test/cmdlineTests/yul_optimizer_erc7201_param_memory/input.sol
// ported-from: test/cmdlineTests/yul_optimizer_erc7201_param_calldata/input.sol

contract Erc7201Builtin {
    string namespace;

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

    function calldataParam(string calldata id) public pure returns (uint256) {
        return erc7201(id);
    }

    function storageValue() public view returns (uint256) {
        return erc7201(namespace);
    }
}
