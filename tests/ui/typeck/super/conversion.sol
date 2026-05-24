//@compile-flags: -Ztypeck

// ported-from: test/libsolidity/syntaxTests/conversion/convert_to_super_empty.sol

contract C {
    function cannotConvertToSuper() public {
        super(); //~ ERROR: cannot convert to the super type
    }
}
