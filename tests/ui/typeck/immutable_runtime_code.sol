// ported-from: test/libsolidity/syntaxTests/immutable/runtimeCode.sol
// ported-from: test/libsolidity/syntaxTests/immutable/runtimeCodeInheritance.sol

contract DirectImmutable {
    address public immutable user = address(0);
}

contract BaseImmutable {
    address public immutable user = address(0);
}

contract InheritedImmutable is BaseImmutable {}

contract Test {
    function direct() public pure returns (bytes memory) {
        return type(DirectImmutable).runtimeCode;
        //~^ ERROR: `runtimeCode` is not available for contracts containing immutable variables
    }

    function inherited() public pure returns (bytes memory) {
        return type(InheritedImmutable).runtimeCode;
        //~^ ERROR: `runtimeCode` is not available for contracts containing immutable variables
    }
}
