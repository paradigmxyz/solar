// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_conflict_2.sol
// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_conflict_3.sol
// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_conflict_4.sol
// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_conflict_5.sol
// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_conflict_6.sol
// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_func_param_6.sol
// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_func_type_param_6.sol
// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_local_6.sol
// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_local_7.sol
// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_nested_6.sol
// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_nested_7.sol
// ported-from: test/libsolidity/syntaxTests/parsing/mapping_with_names_struct_member_6.sol

contract Conflict2 {
    mapping(address owner => address owner) owner; //~ ERROR: conflicting parameter name `owner` in mapping
}

contract Conflict3 {
    mapping(address owner => mapping(address owner => address owner)) owner;
    //~^ ERROR: conflicting parameter name `owner` in mapping
    //~| ERROR: conflicting parameter name `owner` in mapping
    //~| ERROR: conflicting parameter name `owner` in mapping
}

contract Conflict4 {
    mapping(address owner => mapping(address owner => address hello)) world; //~ ERROR: conflicting parameter name `owner` in mapping
}

contract Conflict5 {
    mapping(address owner => mapping(address hello => address owner)) world; //~ ERROR: conflicting parameter name `owner` in mapping
}

contract Conflict6 {
    mapping(address hello => mapping(address owner => address owner)) world; //~ ERROR: conflicting parameter name `owner` in mapping
}

contract FuncParam6 {
    function _main(mapping(uint nameSame => mapping(uint name2 => mapping(uint nameSame => uint name3) name4) name5) storage map) internal { //~ ERROR: conflicting parameter name `nameSame` in mapping
        map[1][2][3] = 4;
    }
}

contract FuncTypeParam6 {
    function(mapping(uint nameSame => mapping(uint name2 => mapping(uint nameSame => uint name3) name4) name5) storage) internal stateVariableName; //~ ERROR: conflicting parameter name `nameSame` in mapping
}

contract Local6 {
    mapping(uint name1 => mapping(uint name2 => uint name3) name4) map;

    function main() external {
        mapping(uint nameSame => mapping(uint name2 => uint nameSame) name4) storage _map = map; //~ ERROR: conflicting parameter name `nameSame` in mapping
        _map[1][2] = 3;
    }
}

contract Local7 {
    mapping(uint nameSame => mapping(uint name1 => mapping(uint nameSame => uint name3) name6) name4) map; //~ ERROR: conflicting parameter name `nameSame` in mapping

    function main() external {
        mapping(uint nameSame => mapping(uint name1 => mapping(uint nameSame => uint name3) name6) name4) storage _map = map; //~ ERROR: conflicting parameter name `nameSame` in mapping
        _map[1][2][3] = 4;
    }
}

contract Nested6 {
    mapping(uint nameSame => mapping(uint name1 => mapping(uint nameSame => uint name2) name3) name4) name5; //~ ERROR: conflicting parameter name `nameSame` in mapping
}

contract Nested7 {
    mapping(uint nameSame => mapping(uint name1 => mapping(uint nameSame => uint name3) name6) name4) public name5; //~ ERROR: conflicting parameter name `nameSame` in mapping
}

contract StructMember6 {
    struct Person {
        mapping(uint nameSame => mapping(uint name1 => mapping(uint nameSame => uint name2) name3) name4) name5; //~ ERROR: conflicting parameter name `nameSame` in mapping
    }
}
