//@ codegen-matrix: standard
//@ run-call: quote false, 3 => 8
//@ run-call: quote true, 3 => 13
//@ run-call: invoke 4 => 9
//@ run-call: callStored 2 => 7
//@ run-call: writeThrough 6 => 6

contract FunctionPointerStorageReference {
    struct S {
        uint256 x;
    }

    S s;
    function(S storage, uint256) view returns (uint256) stored;

    constructor() {
        s.x = 5;
        stored = add;
    }

    function quote(bool twice, uint256 v) external view returns (uint256) {
        function(S storage, uint256) view returns (uint256) f = twice ? addTwice : add;
        return f(s, v);
    }

    function invoke(uint256 v) external returns (uint256) {
        function(S storage, uint256) internal f = bump;
        f(s, v);
        return s.x;
    }

    function callStored(uint256 v) external view returns (uint256) {
        return stored(s, v);
    }

    function writeThrough(uint256 v) external returns (uint256) {
        function(S storage) internal pure returns (S storage) f = identity;
        f(s).x = v;
        return s.x;
    }

    function add(S storage p, uint256 v) internal view returns (uint256) {
        return p.x + v;
    }

    function addTwice(S storage p, uint256 v) internal view returns (uint256) {
        return 2 * p.x + v;
    }

    function bump(S storage p, uint256 v) internal {
        p.x += v;
    }

    function identity(S storage p) internal pure returns (S storage) {
        return p;
    }
}
