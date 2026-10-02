//@ codegen-matrix: standard
//@ run-call: branch true, false, 23 => 26
//@ run-call: branch true, true, 23 => 26
//@ run-call: branch false, true, 23 => 24
//@ run-call: branch false, false, 23 => 99
//@ run-call: transientRead true, false, 23 => 23
//@ run-call: transientRead false, true, 23 => 0
//@ run-call: transientRead false, false, 23 => 99
contract CriticalLoadEdges {
    uint256 private slot = 17;
    function branch(bool store, bool join, uint256 value) external returns (uint256 result) {
        if (store) { slot = value; result = 3; }
        else { if (!join) return 99; result = 7; }
        unchecked { return slot + result; }
    }
    function transientRead(bool store, bool join, uint256 value) external returns (uint256 result) {
        assembly {
            switch store
            case 1 { tstore(5, value) }
            default { if iszero(join) { mstore(0, 99) return(0, 32) } }
            result := tload(5)
        }
    }
}
