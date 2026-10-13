// solc keeps the exact fraction of a literal division, so these operations are valid there.
// NOTE: we still truncate the fraction, so they do not compute solc's values yet.
contract C {
    function div() public pure returns (uint256) {
        return 1 / (1 / 2);
    }

    function rem() public pure returns (uint256) {
        return 1 % ((1 / 2) * 2);
    }

    function shift() public pure returns (uint256) {
        return 1 >> (4294967296 + (-1 / 2) * 2);
    }

    function nested() public pure returns (uint256) {
        return 1 / (1 / 2) + 1;
    }

    function exactDiv() public pure returns (uint256) {
        return 1 / (2 / 2 - 1); //~ ERROR: cannot apply builtin operator `/`
    }

    function fractionalDividend() public pure returns (uint256) {
        return (1 / 2 + 1) / 0; //~ ERROR: cannot apply builtin operator `/`
    }
}
