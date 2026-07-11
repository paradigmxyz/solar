// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/029_create_abstract_contract.sol

abstract contract A {
    function a() public virtual;
}

contract C {
    function f() public {
        new A(); //~ ERROR: cannot instantiate abstract contracts
    }
}
