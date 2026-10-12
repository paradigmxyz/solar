// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/typeChecking/library_instances.sol

library X { }

contract Y {
    X abc; //~ ERROR: invalid use of a library name
    function foo(X param) private view //~ ERROR: invalid use of a library name
    {
        X ofg; //~ ERROR: invalid use of a library name
        ofg = abc;
    }
}
