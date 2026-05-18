//@compile-flags: -Ztypeck

contract C {
    uint256 state;
    uint256[] stateArray;
    uint256 constant constantValue = 1;
    uint256 immutable immutableValue = 1;

    function positive(
        uint256 local,
        uint256[] calldata data,
        function() external returns (uint256) extFn
    ) external {
        uint256[] storage storageRef = stateArray;
        bytes memory memoryBytes = hex"1234";
        bool ok;
        assembly {
            let scratch := 0
            scratch := add(local, 1)
            pop(scratch)
            ok := iszero(0)
            pop(ok)
            pop(memoryBytes)

            pop(state.slot)
            pop(state.offset)
            pop(storageRef.slot)
            pop(storageRef.offset)
            pop(constantValue)
            storageRef.slot := state.slot

            pop(data.offset)
            pop(data.length)
            data.offset := add(data.offset, 32)
            data.length := sub(data.length, 1)

            pop(extFn.address)
            pop(extFn.selector)
            pop(add(1, 2))
        }
    }

    function helper() internal returns (uint256) {
        return 1;
    }

    function negative(
        uint256 local,
        uint256[] calldata data,
        function() external returns (uint256) extFn
    ) external {
        uint256[] storage storageRef = stateArray;
        function() internal returns (uint256) intFn = helper;
        assembly {
            function pair() -> a, b {
                a := 1
                b := 2
            }

            add(1, 2) //~ ERROR: inline assembly expression statements cannot return values
            pair() //~ ERROR: inline assembly expression statements cannot return values
            pop(state) //~ ERROR: only local variables are supported in inline assembly
            state := 1 //~ ERROR: only local variables are supported in inline assembly
            constantValue := 1 //~ ERROR: cannot assign to a constant variable
            pop(immutableValue) //~ ERROR: assembly access to immutable variables is not supported
            pop(state.length) //~ ERROR: storage variables only support `.slot` and `.offset`
            state.slot := 1 //~ ERROR: state variables cannot be assigned to in inline assembly
            pop(storageRef) //~ ERROR: storage reference variables need a suffix in inline assembly
            storageRef.offset := 1 //~ ERROR: only `.slot` can be assigned to
            pop(storageRef.length) //~ ERROR: storage variables only support `.slot` and `.offset`
            pop(data) //~ ERROR: calldata variables need a suffix in inline assembly
            pop(data.slot) //~ ERROR: calldata variables only support `.offset` and `.length`
            pop(local.slot) //~ ERROR: suffix `.slot` is not supported by this variable or type
            pop(extFn.slot) //~ ERROR: function pointer variables only support `.selector` and `.address`
            pop(intFn.selector) //~ ERROR: only external function pointer variables support `.selector` and `.address`
        }
    }
}
