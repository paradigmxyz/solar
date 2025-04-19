contract A {
    constructor() {
        function(uint256) view returns (uint256) a;

        function(uint256) view returns (uint256 j) c; //~ERROR: return parameters in function types may not be named
        //~^WARN: named function type parameters are deprecated

        function(uint256 k) view returns (uint256) d;
        //~^WARN: named function type parameters are deprecated
    }

    function aFunc(uint256 u) public returns (uint256 y) {
        y = u;
    }
}
