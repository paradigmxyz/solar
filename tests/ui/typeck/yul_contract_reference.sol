library L {}

contract D {}

contract C {
    function f() public pure {
        assembly {
            let x := L
            let y := D //~ ERROR: expected a library
        }
    }
}
