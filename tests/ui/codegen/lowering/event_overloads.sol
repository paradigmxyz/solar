//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

// Emitting an overloaded event must use the overload selected by the type
// checker, not the first declared candidate: each emit below must hash its
// own signature into topic0 and encode its own parameter list.

contract EventOverloads {
    event Transfer(uint256 amount);
    event Transfer(address to, uint256 amount);

    // CHECK-LABEL: fn @emitBoth{{[( ]}}
    // CHECK: log1 {{[^,]+}}, 32, 0x248dd4076d0a389d795107efafd558ce7f31ae37b441ccb9a599c60868f480d5
    // CHECK: log1 {{[^,]+}}, 64, 0x69ca02dd4edd7bf0a4abb9ed3b7af3f14778db5d61921c7dc7cd545266326de2
    function emitBoth(address to, uint256 amount) external {
        // keccak256("Transfer(uint256)"), one data word.
        emit Transfer(amount);
        // keccak256("Transfer(address,uint256)"), two data words.
        emit Transfer(to, amount);
    }
}
