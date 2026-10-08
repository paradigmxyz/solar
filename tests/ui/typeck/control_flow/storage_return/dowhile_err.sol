// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/dowhile_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        do {
            break;
            c = s;
        } while(false);
    }
    function g() internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        do {
            if (s.f) {
                continue;
                c = s;
            }
            else {
            }
        } while(false);
    }
    function h() internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        do {
            if (s.f) {
                break;
            }
            else {
                c = s;
            }
        } while(false);
    }
    function i() internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        do {
            if (s.f) {
                continue;
            }
            else {
                c = s;
            }
        } while(false);
    }
    function j() internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        do {
            continue;
            c = s;
        } while(false);
    }
}
