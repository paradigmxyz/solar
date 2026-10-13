// ported-from: test/libsolidity/syntaxTests/controlFlow/localCalldataVariables/if_declaration_err.sol
contract C {
    function f(uint[] calldata _c) public pure {
        uint[] calldata c;
        if (_c[2] > 10)
            c = _c;
        c[2]; //~ ERROR: this variable is of calldata pointer type and can be accessed
    }
}
