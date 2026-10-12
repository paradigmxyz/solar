// ported-from: test/libsolidity/syntaxTests/errors/hash_collision.sol
contract test {
    error gsf();
    error tgeo(); //~ ERROR: error signature hash collision
}
