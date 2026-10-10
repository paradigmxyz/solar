// ported-from: test/libsolidity/syntaxTests/errors/hash_collision_external.sol
library L {
    error gsf();
}
contract test {
    error tgeo(); //~ ERROR: error signature hash collision
    function f(bool a) public {
        if (a)
            revert L.gsf();
        else
            revert tgeo();
    }
}
