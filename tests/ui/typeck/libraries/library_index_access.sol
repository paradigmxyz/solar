library C {
    function f() view public {
        C[0]; //~ ERROR: index access for library types is not possible
        C[1]; //~ ERROR: index access for library types is not possible
    }
}
