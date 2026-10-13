library C {
    function f() view public {
        C[0]; //~ ERROR: index access for library types is not possible
        //~^ ERROR: array length must be greater than zero
        C[1]; //~ ERROR: index access for library types is not possible
    }
}
