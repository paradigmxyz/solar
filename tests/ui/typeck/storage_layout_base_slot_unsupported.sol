// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specified_by_attached_library_function.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/string.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/hex_string.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_fractional_number.sol

library L {
    function f(uint x) public pure returns (uint) {
        return x * 2;
    }
}

contract AttachedFunction layout at 2.f() { //~ ERROR: failed to evaluate constant: unsupported expression
    using L for *;
}

contract StringLiteral layout at "MyLayoutBase" {} //~ ERROR: failed to evaluate constant: unsupported literal
contract HexStringLiteral layout at hex"616263" {} //~ ERROR: failed to evaluate constant: unsupported literal
contract FractionalNumber layout at 4.2 {} //~ ERROR: failed to evaluate constant: unsupported literal
contract LeadingFractionalNumber layout at .1 {} //~ ERROR: failed to evaluate constant: unsupported literal
contract NegativeExponent layout at 42e-10 {} //~ ERROR: failed to evaluate constant: unsupported literal
contract UnderscoredNegativeExponent layout at 1_7e-10 {} //~ ERROR: failed to evaluate constant: unsupported literal
