// ported-from: test/libsolidity/syntaxTests/dataLocations/transient_value_type_state_variables.sol
// ported-from: test/libsolidity/syntaxTests/dataLocations/transient_function_type.sol
// ported-from: test/libsolidity/syntaxTests/dataLocations/transient_dynamic_array_state_variable.sol
// ported-from: test/libsolidity/syntaxTests/dataLocations/transient_fixed_array_state_variable.sol
// ported-from: test/libsolidity/syntaxTests/dataLocations/transient_struct_state_variable.sol
// ported-from: test/libsolidity/syntaxTests/dataLocations/transient_mapping_state_variable.sol

// Transient storage holds one slot per variable, so only value types can live there.

type Price is uint128;

contract D {}

contract C {
    enum E { A, B }

    struct S {
        uint256 x;
    }

    address transient a;
    bool transient b;
    D transient d;
    uint256 transient x;
    bytes32 transient y;
    E transient e;
    Price transient p;
    function () transient f;
    function (uint256) external transient g;

    bytes transient bs; //~ ERROR: transient data location is only supported for value types
    string transient st; //~ ERROR: transient data location is only supported for value types
    uint256[] transient arr; //~ ERROR: transient data location is only supported for value types
    uint256[3] transient farr; //~ ERROR: transient data location is only supported for value types
    S transient s; //~ ERROR: transient data location is only supported for value types
    mapping(uint256 => uint256) transient m; //~ ERROR: transient data location is only supported for value types
}
