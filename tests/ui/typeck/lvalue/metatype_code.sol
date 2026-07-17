// ported-from: test/libsolidity/syntaxTests/metaTypes/codeIsNoLValue.sol

contract MetaTypeMemberLvalues {
    function f() public pure {
        type(MetaTypeMemberLvalues).creationCode = new bytes(6); //~ ERROR: expression has to be an lvalue
        type(MetaTypeMemberLvalues).runtimeCode = new bytes(6); //~ ERROR: expression has to be an lvalue
    }
}
