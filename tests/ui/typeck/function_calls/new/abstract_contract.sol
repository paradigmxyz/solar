//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/029_create_abstract_contract.sol

contract Base { //~ ERROR: contract `Base` has unimplemented functions
    function foo() public virtual;
}

contract Derived {
    Base b;

    function foo() public {
        b = new Base();
    }
}
