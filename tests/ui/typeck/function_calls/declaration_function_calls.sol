// ported-from: test/libsolidity/syntaxTests/types/contractTypeType/members/call_function_via_contract_name.sol
// ported-from: test/libsolidity/syntaxTests/freeFunctions/free_call_via_contract_type.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/145_external_base_visibility.sol
// ported-from: test/libsolidity/syntaxTests/types/contractTypeType/members/base_contract_invalid.sol

contract A {
    function f() external {}
    function g() external pure {}
    function h() public pure {}
}

contract B {
    function i() external {
        A.f(); //~ ERROR: cannot call function via contract type name
        A.g(); //~ ERROR: cannot call function via contract type name
        A.h(); //~ ERROR: cannot call function via contract type name
    }
}

function freeFunction() {
    A.f(); //~ ERROR: cannot call function via contract type name
}

contract Base {
    function externalBase() external {}
}

contract Derived is Base {
    function derivedCall() public {
        Base.externalBase(); //~ ERROR: cannot call function via contract type name
    }
}

contract BaseMembers {
    function externalMember() external {}
}

contract DerivedMembers is BaseMembers {
    function memberCall() public {
        BaseMembers.externalMember(); //~ ERROR: cannot call function via contract type name
    }
}
