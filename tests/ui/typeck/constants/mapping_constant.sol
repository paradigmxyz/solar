// ported-from: test/libsolidity/syntaxTests/constants/mapping_constant.sol
mapping(uint => uint) constant b = b; //~ ERROR: only constants of value type and byte array type are implemented
