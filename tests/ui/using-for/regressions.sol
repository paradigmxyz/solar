//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/enums/using_contract_enums_with_explicit_contract_name.sol
// ported-from: test/libsolidity/semanticTests/enums/using_inherited_enum_excplicitly.sol

contract EnumOwner {
    enum Choice {
        A,
        B,
        C
    }

    function answer() public pure returns (EnumOwner.Choice ret) {
        ret = EnumOwner.Choice.B;
    }
}

contract EnumBase {
    enum Choice {
        A,
        B,
        C
    }
}

contract EnumChild is EnumBase {
    function answer() public pure returns (EnumBase.Choice ret) {
        ret = EnumBase.Choice.B;
    }
}
