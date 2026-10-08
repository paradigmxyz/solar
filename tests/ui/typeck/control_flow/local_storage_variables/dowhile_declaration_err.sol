// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/dowhile_declaration_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view {
        S storage c;
        do {
            break;
            c = s;
        } while(false);
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function g() internal view {
        S storage c;
        do {
            if (s.f) {
                continue;
                c = s;
            }
            else {
            }
        } while(false);
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function h() internal view {
        S storage c;
        do {
            if (s.f) {
                break;
            }
            else {
                c = s;
            }
        } while(false);
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function i() internal view {
        S storage c;
        do {
            if (s.f) {
                continue;
            }
            else {
                c = s;
            }
        } while(false);
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function j() internal view {
        S storage c;
        do {
            continue;
            c = s;
        } while(false);
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
