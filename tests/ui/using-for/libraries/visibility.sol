//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/library_functions_at_file_level.sol
// ported-from: test/libsolidity/syntaxTests/using/library_functions_inside_contract.sol
// ported-from: test/libsolidity/syntaxTests/using/private_library_function_inside_scope.sol

library L {
    function ext(uint256 x) external pure returns (uint256) {
        return x;
    }

    function pub(uint256 x) public pure returns (uint256) {
        return x;
    }

    function inner(uint256 x) internal pure returns (uint256) {
        return x;
    }
}

using L for uint256;
using {L.ext, L.pub, L.inner} for uint256;

contract C {
    using L for uint256;
    using {L.ext, L.pub, L.inner} for uint256;

    function run(uint256 x) public pure returns (uint256) {
        return x.ext() + x.pub() + x.inner();
    }
}

library PrivateScope {
    using {PrivateScope.priv} for uint256;

    function priv(uint256 x) private pure returns (uint256) {
        return x;
    }

    function run(uint256 x) internal pure returns (uint256) {
        return x.priv();
    }
}
