// Function type parameters take their data locations from the visibility of the function type,
// which defaults to internal, not from the enclosing function.

contract C {
    struct S {
        uint256 x;
    }

    struct T {
        function(S storage, S calldata) f;
    }

    function(S storage) internal view returns (S storage) internalVar;
    function(S storage) view returns (uint256) defaultVar;
    function(S storage) external externalVar; //~ ERROR: invalid data location `storage`
    mapping(uint256 => function(S storage) returns (S storage)) map;

    constructor() {
        function(S storage, S calldata) internal a;
        function(S memory) external returns (S calldata) b;
        a;
        b;
    }

    function ext() external pure {
        function(S storage) returns (S storage) a;
        function(S calldata) external b;
        a;
        b;
    }

    function int_() internal pure {
        function(S storage) external a; //~ ERROR: invalid data location `storage`
        function() external returns (S storage) b; //~ ERROR: invalid data location `storage`
        a;
        b;
    }
}
