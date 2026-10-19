//@ codegen-matrix: standard
//@ run-call-fail: fail()
//@ run-call: pass() => 7
// The copy's destination is never read, but `returndatacopy` past the end of
// the return data reverts, so it must stay.
contract ReturndataTrap {
    function fail() external pure returns (uint256) {
        bytes memory unused = new bytes(32);
        assembly { returndatacopy(add(unused, 32), 0, 32) }
        return 7;
    }

    function pass() external pure returns (uint256) {
        bytes memory unused = new bytes(32);
        assembly { returndatacopy(add(unused, 32), 0, 0) }
        return 7;
    }
}
