//@ check-pass
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=MIR

// Emitting an overloaded event must use the overload selected by the type
// checker, not the first declared candidate: each emit below must hash its
// own signature into topic0 and encode its own parameter list.

contract EventOverloads {
    event Transfer(uint256 amount);
    event Transfer(address to, uint256 amount);

    function emitBoth(address to, uint256 amount) external {
        // MIR-LABEL: fn @emitBoth
        // keccak256("Transfer(uint256)"), one data word.
        // MIR: log1 0, 32, 0x248dd4076d0a389d795107efafd558ce7f31ae37b441ccb9a599c60868f480d5
        emit Transfer(amount);
        // keccak256("Transfer(address,uint256)"), two data words.
        // MIR: log1 0, 64, 0x69ca02dd4edd7bf0a4abb9ed3b7af3f14778db5d61921c7dc7cd545266326de2
        emit Transfer(to, amount);
    }
}
