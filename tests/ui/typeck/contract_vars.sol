function f() {}
event E1();
error E2();

contract C {
    f a; //~ ERROR: name has to refer to a valid user-defined type
    E1 b; //~ ERROR: name has to refer to a valid user-defined type
    E2 c; //~ ERROR: name has to refer to a valid user-defined type
}
