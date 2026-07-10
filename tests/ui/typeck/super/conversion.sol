//@ compile-flags: -Ztypeck

// ported-from: test/libsolidity/syntaxTests/conversion/convert_to_super_empty.sol
// ported-from: test/libsolidity/syntaxTests/conversion/convert_to_super_nonempty.sol
// ported-from: test/libsolidity/syntaxTests/conversion/not_allowed_conversion_from_super.sol
// ported-from: test/libsolidity/syntaxTests/types/address/super_to_address.sol

contract Empty {
    function f() public pure {
        super().x; //~ ERROR: cannot convert to the super type
    }
}

contract Nonempty {
    function f() public pure {
        super(this).f(); //~ ERROR: cannot convert to the super type
    }
}

contract S {
    int256 o;

    function foo() public returns (int256) {
        return o = 3;
    }
}

contract B is S {
    function fii() public {
        o = S(super).foo(); //~ ERROR: invalid explicit type conversion
    }
}

contract AddressConversion {
    function f() public pure {
        address(super); //~ ERROR: invalid explicit type conversion
    }
}
