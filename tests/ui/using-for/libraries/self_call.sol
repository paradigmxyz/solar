// ported-from: test/libsolidity/syntaxTests/using/library_functions_attached_at_file_level_used_inside_library.sol

library L {
    using {L.ext, L.pub, L.inner, L.priv} for uint256;

    function ext(uint256 x) external pure returns (uint256) {
        return x;
    }

    function pub(uint256 x) public pure returns (uint256) {
        return x;
    }

    function inner(uint256 x) internal pure returns (uint256) {
        return x;
    }

    function priv(uint256 x) private pure returns (uint256) {
        return x;
    }

    function run(uint256 x) internal pure returns (uint256) {
        return x.ext() + x.pub() + x.inner() + x.priv();
        //~^ ERROR: libraries cannot call their own functions externally
        //~| ERROR: libraries cannot call their own functions externally
    }
}
