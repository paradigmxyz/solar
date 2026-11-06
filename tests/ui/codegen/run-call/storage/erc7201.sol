//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: literal => 0x183a6125c38840424c4a85fa12bab2ab606c4b6d0e7cc73c0c06ba5300eab500
//@ run-call: zeroLastByteLiteral => 0x6d0d983459328e82eacb1bf2d6fadfa38a6896e9d4cbfe0e1aa41c6281bab00
//@ run-call: memoryParam => 0x183a6125c38840424c4a85fa12bab2ab606c4b6d0e7cc73c0c06ba5300eab500
//@ run-call: calldataParam "example.main" => 0x183a6125c38840424c4a85fa12bab2ab606c4b6d0e7cc73c0c06ba5300eab500
//@ run-call: storageValue => 0x4318a0031e4d2f411be9017543511db04d79cf580aaff6bae7539a4a49eacc00
//@ run-call: constantDivision => 0x81375b7412d6ac0c418d7535b9390e3cacec3cf04d4426959579371004e3c55
// ported-from: test/cmdlineTests/yul_optimizer_erc7201_literal_comptime_evaluation/input.sol
// ported-from: test/cmdlineTests/yul_optimizer_erc7201_param_memory/input.sol

contract Erc7201RunCall {
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

    // The slot is a `uint256`, so dividing it is integer division.
    uint256 constant SLOT_THIRD = erc7201("example.main") / 3;

    function constantDivision() public pure returns (uint256) {
        return SLOT_THIRD;
    }
}
