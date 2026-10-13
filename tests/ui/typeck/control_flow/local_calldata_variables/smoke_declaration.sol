// ported-from: test/libsolidity/syntaxTests/controlFlow/localCalldataVariables/smoke_declaration.sol
contract C {
    function g() internal pure returns (bytes calldata) {
        return msg.data;
    }
    function h(uint[] calldata _c) internal pure {
        uint[] calldata c;
        c = _c;
        c[2];
    }
    function i(uint[] calldata _c) internal pure {
        uint[] calldata c;
        (c) = _c;
        c[2];
    }
}
