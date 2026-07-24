contract C {
    struct S {
        uint256 x;
    }

    function f(uint256 n) public pure {
        bytes memory b = new bytes(n);
        string memory s = new string(n);
        uint256[] memory a = new uint256[](n);
        S[] memory structs = new S[](n);
        uint256[][] memory nested = new uint256[][](n);
    }
}
