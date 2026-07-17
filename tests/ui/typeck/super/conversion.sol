// ported-from: test/libsolidity/syntaxTests/conversion/convert_to_super_empty.sol
// ported-from: test/libsolidity/syntaxTests/conversion/convert_to_super_nonempty.sol
// ported-from: test/libsolidity/syntaxTests/conversion/not_allowed_conversion_from_super.sol
// ported-from: test/libsolidity/syntaxTests/types/address/super_to_address.sol

contract C {
    function cannotConvertToSuper() public {
        super(); //~ ERROR: cannot convert to the super type
        super(this); //~ ERROR: cannot convert to the super type
        C(super); //~ ERROR: invalid explicit type conversion
        address(super); //~ ERROR: invalid explicit type conversion
    }
}
