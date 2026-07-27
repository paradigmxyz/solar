contract C {
    // Param is an internal function type.
    function(function () internal) external x; //~ ERROR: internal type cannot be used for external function type

    // Return is an internal function type.
    function() external returns (function () internal) y; //~ ERROR: internal type cannot be used for external function type

    // Nested external function type still rejects an internal parameter.
    function(function(function () internal) external) external nested; //~ ERROR: internal type cannot be used for external function type

    // Valid: external-in-external is OK.
    function(function () external) external ok;
}
