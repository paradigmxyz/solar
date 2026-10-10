// ported-from: test/libsolidity/syntaxTests/array/contract_index_access.sol

contract C {
    function f() view public { //~ WARN: function state mutability can be restricted to pure
        C[0];
    }
}
