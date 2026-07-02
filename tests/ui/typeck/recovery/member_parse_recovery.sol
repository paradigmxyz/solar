//@ compile-flags: -Ztypeck

contract C {
    struct S {
        uint256 a;
    }

    function missingMemberNameStatement() public {
        S memory s = S(1);
        s.; //~ ERROR: expected identifier, found `;`
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function missingMemberNameExpression() public {
        S memory s = S(1);
        uint256 x = s.; //~ ERROR: expected identifier, found `;`
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function doubleDotMember() public {
        S memory s = S(1);
        uint256 x = s..a; //~ ERROR: expected identifier, found `.`
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function missingMemberNameBeforeCall() public {
        S memory s = S(1);
        s.(); //~ ERROR: expected identifier, found `(`
        uint8 y = 300; //~ ERROR: mismatched types
    }
}
