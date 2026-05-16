contract C {
    function f() public returns (uint256 x) {
        // Ported from solc syntaxTests/viewPureChecker/inline_assembly_instructions_allowed.sol.
        assembly {
            pop(calldatasize())
            calldatacopy(0, 1, 2)
            pop(codesize())
            codecopy(0, 1, 2)
            pop(extcodesize(0))
            extcodecopy(0, 1, 2, 3)
            pop(returndatasize())
            returndatacopy(0, 1, 2)
            x := add(x, 1)
        }
    }
}
