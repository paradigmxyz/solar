//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/implementing_operator_with_contract_function_at_file_level.sol
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/implementing_operator_with_library_function_at_file_level.sol

type ContractInt is int256;
type ExternalInt is int128;
type InternalInt is int128;
type PublicInt is int128;

contract C {
    function add(ContractInt, ContractInt) public pure returns (ContractInt) {
        return ContractInt.wrap(0);
    }
}

library ExternalLibrary {
    function binaryOperator(ExternalInt, ExternalInt) external pure returns (ExternalInt) {}
    function unaryOperator(ExternalInt) external pure returns (ExternalInt) {}
}

library InternalLibrary {
    function binaryOperator(InternalInt, InternalInt) internal pure returns (InternalInt) {}
    function unaryOperator(InternalInt) internal pure returns (InternalInt) {}
}

library PublicLibrary {
    function binaryOperator(PublicInt, PublicInt) public pure returns (PublicInt) {}
    function unaryOperator(PublicInt) public pure returns (PublicInt) {}
}

using {C.add as +} for ContractInt global; //~ ERROR: only file-level functions and library functions
//~^ ERROR: only pure free functions can be used to define operators
using {ExternalLibrary.binaryOperator as +} for ExternalInt global; //~ ERROR: only pure free functions can be used to define operators
using {ExternalLibrary.unaryOperator as -} for ExternalInt global; //~ ERROR: only pure free functions can be used to define operators
using {InternalLibrary.binaryOperator as +} for InternalInt global; //~ ERROR: only pure free functions can be used to define operators
using {InternalLibrary.unaryOperator as -} for InternalInt global; //~ ERROR: only pure free functions can be used to define operators
using {PublicLibrary.binaryOperator as +} for PublicInt global; //~ ERROR: only pure free functions can be used to define operators
using {PublicLibrary.unaryOperator as -} for PublicInt global; //~ ERROR: only pure free functions can be used to define operators
