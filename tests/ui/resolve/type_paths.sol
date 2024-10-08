import "./type_paths.sol" as self;

contract C {
    struct S {
        uint x;
    }

    function f() public {
        S memory a;
        C.S memory b;
        self.C.S memory c;
    }
}
