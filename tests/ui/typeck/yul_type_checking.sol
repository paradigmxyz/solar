//@compile-flags: -Ztypeck

type U256 is uint256;
type Word is bytes32;

contract C {
    uint256 state;
    uint256[] stateArray;
    U256 udvtState;
    uint256 constant constantValue = 1;
    uint256 constant constantExpr = 1 + 2;
    bool constant boolConstant = true;
    address constant addressConstant = 0x1234567890123456789012345678901234567890;
    bytes32 constant bytes32Constant =
        hex"1234567890123456789012345678901234567890123456789012345678901234";
    bytes32 constant convertedConstant = bytes32(uint256(1));
    string constant stringConstant = "abc";
    uint256 immutable immutableValue = 1;

    function positive(
        uint256 local,
        bytes32 word,
        U256 udvt,
        Word udvtWord,
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
            let wordValue := word
            word := add(wordValue, 1)
            let udvtValue := udvt
            udvt := add(udvtValue, 1)
            let udvtWordValue := udvtWord
            udvtWord := add(udvtWordValue, 1)
            ok := iszero(0)
            pop(ok)
            pop(memoryBytes)

            pop(state.slot)
            pop(state.offset)
            pop(udvtState.slot)
            pop(udvtState.offset)
            pop(storageRef.slot)
            pop(storageRef.offset)
            pop(constantValue)
            pop(constantExpr)
            pop(boolConstant)
            pop(addressConstant)
            pop(bytes32Constant)
            storageRef.slot := state.slot

            pop(data.offset)
            pop(data.length)
            data.offset := add(data.offset, 32)
            data.length := sub(data.length, 1)

            pop(extFn.address)
            pop(extFn.selector)
            extFn.address := 0
            extFn.selector := 0
            pop(add(1, 2))
        }
    }

    function helper() internal returns (uint256) {
        return 1;
    }

    // TODO: enable once UDVT `.wrap` is implemented in Solar.
    // U256 constant udvtConstant = U256.wrap(1);
    // function udvt_constant() external {
    //     assembly {
    //         pop(udvtConstant)
    //     }
    // }

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

            pop(udvtState) //~ ERROR: only local variables are supported in inline assembly

            udvtState := 1 //~ ERROR: only local variables are supported in inline assembly

            constantValue := 1 //~ ERROR: cannot assign to a constant variable

            pop(convertedConstant) //~ ERROR: only direct number constants are supported in inline assembly

            pop(stringConstant) //~ ERROR: only direct number constants are supported in inline assembly

            pop(immutableValue) //~ ERROR: assembly access to immutable variables is not supported

            pop(immutableValue.slot) //~ ERROR: assembly access to immutable variables is not supported

            pop(state.length) //~ ERROR: storage variables only support `.slot` and `.offset`

            state.slot := 1 //~ ERROR: state variables cannot be assigned to in inline assembly

            pop(storageRef) //~ ERROR: storage reference variables need a suffix in inline assembly

            storageRef.offset := 1 //~ ERROR: only `.slot` can be assigned to

            pop(storageRef.length) //~ ERROR: storage variables only support `.slot` and `.offset`

            pop(data) //~ ERROR: calldata variables need a suffix in inline assembly

            pop(data.slot) //~ ERROR: calldata variables only support `.offset` and `.length`

            pop(local.slot) //~ ERROR: suffix `.slot` is not supported by this variable or type

            pop(extFn) //~ ERROR: only types that use one stack slot are supported

            extFn := 0 //~ ERROR: only types that use one stack slot are supported

            pop(extFn.slot) //~ ERROR: function pointer variables only support `.selector` and `.address`

            pop(intFn.selector) //~ ERROR: only external function pointer variables support `.selector` and `.address`

            pop(helper) //~ ERROR: access to functions is not allowed in inline assembly
        }
    }
}
