//@compile-flags: -Ztypeck

contract C {
    // --- Functions with different mutabilities ---
    function pure_fn() internal pure {}

    function view_fn() internal view {}

    function nonpayable_fn() internal {}

    function payable_fn() internal payable {}

    function external_pure_fn() external pure {}

    // --- Functions with different signatures ---
    function pure_fn_param(uint256) internal pure {}

    function pure_fn_returns() internal pure returns (uint256) {
        return 1;
    }

    function test_all() internal {
        // --- ALLOWED: State mutability conversions (internal) ---
        function() internal view f_view;
        f_view = pure_fn; // pure -> view

        function() internal f_nonpayable;
        f_nonpayable = pure_fn; // pure -> nonpayable
        f_nonpayable = view_fn; // view -> nonpayable
        f_nonpayable = payable_fn; // payable -> nonpayable

        // --- DISALLOWED: State mutability conversions (internal) ---
        function() internal pure f_pure;
        f_pure = view_fn; //~ ERROR: mismatched types
        f_pure = nonpayable_fn; //~ ERROR: mismatched types
        f_pure = payable_fn; //~ ERROR: mismatched types

        f_view = nonpayable_fn; //~ ERROR: mismatched types
        f_view = payable_fn; //~ ERROR: mismatched types

        function() internal payable f_payable;
        f_payable = pure_fn; //~ ERROR: mismatched types
        f_payable = view_fn; //~ ERROR: mismatched types
        f_payable = nonpayable_fn; //~ ERROR: mismatched types

        // --- DISALLOWED: Internal/External mismatch ---
        f_pure = this.external_pure_fn; //~ ERROR: mismatched types
        function() external pure f_external;
        f_external = pure_fn; //~ ERROR: mismatched types

        // --- DISALLOWED: Signature mismatch ---
        f_pure = pure_fn_param; //~ ERROR: mismatched types
        f_pure = pure_fn_returns; //~ ERROR: mismatched types

        function(uint256) internal pure f_with_param;
        f_with_param = pure_fn; //~ ERROR: mismatched types

        function() internal pure returns (uint256) f_with_return;
        f_with_return = pure_fn; //~ ERROR: mismatched types
    }
}
