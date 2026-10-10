// ported-from: test/libsolidity/syntaxTests/constructor/constructor_internal_function.sol
// ported-from: test/libsolidity/syntaxTests/constructor/constructor_internal_function_abstract.sol
// ported-from: test/libsolidity/syntaxTests/constructor/constructor_storage.sol
// ported-from: test/libsolidity/syntaxTests/constructor/constructor_storage_abstract.sol
// ported-from: test/libsolidity/syntaxTests/functionCalls/calldata_struct_array_argument_with_internal_data_type_inside_as_constructor_parameter.sol
// ported-from: test/libsolidity/syntaxTests/types/mapping/constructor_parameter.sol

contract InternalFunction {
    constructor(function() internal) {} //~ ERROR: types containing internal function pointers cannot be constructor parameters
}

abstract contract InternalFunctionAbstract {
    constructor(function() internal) {}
}

contract Storage {
    constructor(uint[] storage a) {} //~ ERROR: this parameter has a type that can only be used internally
}

abstract contract StorageAbstract {
    constructor(uint[] storage a) {}
}

contract StructArrayStorage {
	struct S {
		function() a;
	}
	constructor (S[2] storage) public {}
    //~^ ERROR: this parameter has a type that can only be used internally
    //~| WARN: visibility for constructor is ignored
}

contract MappingArrayStorage {
    constructor (mapping (uint => uint) [] storage) { } //~ ERROR: this parameter has a type that can only be used internally
}

struct Recursive {
    Recursive[] children;
}

contract RecursiveMemory {
    constructor(Recursive memory r) {} //~ ERROR: recursive types cannot be constructor parameters
}

abstract contract RecursiveMemoryAbstract {
    constructor(Recursive memory r) {}
}

contract ExternalFunction {
    constructor(function() external f, uint[] memory a) {}
}
