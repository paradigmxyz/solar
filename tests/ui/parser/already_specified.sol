contract C
    is A //~ ERROR: unresolved symbol `A`
    is B //~ ERROR: base contracts already specified
    layout at 0
    layout at 1 //~ ERROR: storage layout already specified
{
    function f() //~ ERROR: Function has override specified but does not override anything
    public
    private //~ ERROR: visibility already specified

    view
    pure //~ ERROR: state mutability already specified

    virtual
    virtual //~ ERROR: virtual already specified

    override
    override //~ ERROR: override already specified
    {}

    uint //~ ERROR: public state variable has override specified but does not override anything
    public
    private //~ ERROR: visibility already specified

    constant
    immutable //~ ERROR: mutability already specified

    virtual //~ ERROR: `virtual` is not allowed here

    override
    override //~ ERROR: override already specified
    x = 0;
}
