//@ run-call: solidity => 7
//@ run-call: yulFunction => 7
contract C {
    uint256 constant K = 7;

    function solidity() public pure returns (uint256) {
        function(uint256 K) external cb;
        //~^ WARN: named function type parameters are deprecated
        cb;
        return K;
    }

    function yulFunction() public pure returns (uint256 r) {
        function(uint256 K) external cb;
        //~^ WARN: named function type parameters are deprecated
        cb;
        assembly {
            function read() -> v {
                v := K
            }
            r := read()
        }
    }
}
