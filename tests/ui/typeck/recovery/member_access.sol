contract C {
    struct S {
        uint256 a;
    }

    function missingStructMember() public {
        S memory s = S(1);
        uint256 x = s.else; //~ ERROR: member `else` not found
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function missingStructMemberCall() public {
        S memory s = S(1);
        uint256 x = s.else(); //~ ERROR: member `else` not found
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function missingPrimitiveMember() public {
        uint256 x = (1).foo; //~ ERROR: member `foo` not found
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function missingTupleMember() public {
        uint256 x = (1, 2).foo; //~ ERROR: member `foo` not found
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function missingMetatypeMember() public {
        uint256 x = type(C).missing; //~ ERROR: member `missing` not found
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function unresolvedReceiverMember() public {
        uint256 x = missing.member; //~ ERROR: unresolved symbol `missing`
        uint8 y = 300; //~ ERROR: mismatched types
    }
}
