//@compile-flags: -Ztypeck

contract C {
    function f() public returns (uint256 x, uint256 y) {
        assembly {
            function pair(a) -> b, c {
                b := add(a, 1)
                c := add(a, 2)
            }

            let a, b := pair(1)
            x, y := pair(add(a, b))
        }
    }
}
