//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specified_by_attached_library_function.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_fractional_number.sol

library L {
    function f(uint x) public pure returns (uint) {
        return x * 2;
    }
}

contract AttachedFunction layout at 2.f() { //~ ERROR: base slot of storage layout must be a compile-time constant expression
    using L for *;
}

contract FractionalDivision layout at 3/2 {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract FractionalNumber layout at 4.2 {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract LeadingFractionalNumber layout at .1 {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract NegativeExponent layout at 42e-10 {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract UnderscoredNegativeExponent layout at 1_7e-10 {} //~ ERROR: base slot of storage layout must evaluate to an integer
