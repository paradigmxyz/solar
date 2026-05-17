//@compile-flags: -Ztypeck

contract C {
    uint256 state;

    function f(uint256 local, uint256[] calldata data) external {
        assembly {
            function pair() -> a, b {
                a := 1
                b := 2
            }

            add(1, 2) //~ ERROR: inline assembly expression statements cannot return values
            pair() //~ ERROR: inline assembly expression statements cannot return values
            pop(state) //~ ERROR: only local variables are supported in inline assembly
            pop(state.length) //~ ERROR: storage variables only support `.slot` and `.offset`
            state.slot := 1 //~ ERROR: state variables cannot be assigned to in inline assembly
            pop(data.slot) //~ ERROR: calldata variables only support `.offset` and `.length`
            pop(local.slot) //~ ERROR: suffix `.slot` is not supported by this variable or type
        }
    }
}
