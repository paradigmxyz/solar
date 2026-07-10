//@compile-flags: -Zcodegen --emit=bin-runtime

// Constant short revert messages share one synthesized `__revert_error`
// helper per module: each `require`/`revert` site passes the length and the
// left-aligned data word instead of materializing and ABI-encoding the string
// in place (~60-90 bytes per site — aave's `Errors.X` constants alone account
// for kilobytes). The constant may be a literal, a local `constant`, or a
// library `constant` reached through a member access. Messages longer than
// one word materialize their resolved bytes and use the generic encoder —
// resolving through `lower_expr` would truncate the constant to one word.
// Revert data is byte-identical to solc 0.8.30 for every shape (verified on
// anvil, including the 33-byte and empty-string edges).

library Errors {
    string public constant SHORT = "39";
    string public constant LONG = "this-is-a-33-byte-long-message!!!";
}

contract R {
    string constant LOCAL = "local-const-msg";

    function viaLibConst(uint256 x) external pure returns (uint256) {
        require(x > 5, Errors.SHORT);
        return x;
    }

    function viaLiteral(uint256 x) external pure returns (uint256) {
        require(x > 5, "literal msg");
        return x;
    }

    function viaLocalConst(uint256 x) external pure returns (uint256) {
        require(x > 5, LOCAL);
        return x;
    }

    function viaLong(uint256 x) external pure returns (uint256) {
        require(x > 5, Errors.LONG);
        return x;
    }

    function viaRevertMsg(uint256 x) external pure returns (uint256) {
        if (x <= 5) {
            revert("revert-path");
        }
        return x;
    }
}
