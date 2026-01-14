//@compile-flags: -Ztypeck
function f() {
//~^ WARN: function state mutability can be restricted to pure
    address payable e = 0x14aF3198B9Dd911fc828434f8D97df0C0Ff979Ee; //~ ERROR: mismatched types

    // ok
    address payable p = payable(0x14aF3198B9Dd911fc828434f8D97df0C0Ff979Ee);
    address a = p;

    address payable p2 = a; //~ ERROR: mismatched types
}
