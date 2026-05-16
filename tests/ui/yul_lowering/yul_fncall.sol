//@compile-flags: -Ztypeck

contract C {
    function f(uint256 a) public returns (uint256 x, uint256 y) {
        assembly {
            function yulfn(p, q) -> r, s {
                r := add(p, q)
                s := sub(p, q)
            }

            x, y := yulfn(a, 7)
            side_effect_only(x)
        }
    }
}
