// References whose type already failed to type check do not report a second error.

contract C {
    Missing constant K = 1; //~ ERROR: unresolved symbol `Missing`
    mapping(uint256 => uint256) stateMap;
    mapping(uint256 => Missing) badMap; //~ ERROR: unresolved symbol `Missing`

    function f(Missing[] calldata cd) public view { //~ ERROR: unresolved symbol `Missing`
        Missing storage ptr; //~ ERROR: unresolved symbol `Missing`
        assembly {
            pop(K)
            pop(cd)
            pop(ptr.slot)
            pop(missing.slot) //~ ERROR: unresolved symbol `missing`
        }
    }

    function assign() public {
        mapping(uint256 => Missing) storage ptr = stateMap; //~ ERROR: unresolved symbol `Missing`
        assembly {
            ptr.slot := 0
            badMap := 0
        }
    }
}
