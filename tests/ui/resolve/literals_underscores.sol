contract LT {
    uint constant _TLOAD_TEST_PAYLOAD1 = 0x6002_601e_613d5c_3d_52_f3;
    uint constant _TLOAD_TEST_PAYLOAD2 = 0x6002_601E_613d5c_3d_52_f3;

    uint a = 1000_; //~ ERROR: invalid use of underscores in number literal
    uint b = 100__0; //~ ERROR: invalid use of underscores in number literal
    uint c = 1_.4e10; //~ ERROR: invalid use of underscores in number literal
    uint d = 3.4_e10; //~ ERROR: invalid use of underscores in number literal
    uint e = 3.4e_10; //~ ERROR: invalid use of underscores in number literal

    uint g = 1_.4e10 + 3.4e_10; //~ ERROR: invalid use of underscores in number literal
    //~^ ERROR: invalid use of underscores in number literal

    uint X1 = 0x1234__1234__1234__123; //~ ERROR: invalid use of underscores in number literal

    uint D1 = 1234_; //~ ERROR: invalid use of underscores in number literal
    uint D2 = 12__34; //~ ERROR: invalid use of underscores in number literal
    uint D3 = 12_e34; //~ ERROR: invalid use of underscores in number literal
    uint D4 = 12e_34; //~ ERROR: invalid use of underscores in number literal

    uint F1 = 3.1415_; //~ ERROR: invalid use of underscores in number literal
    uint F2 = 3__1.4__15; //~ ERROR: invalid use of underscores in number literal
    uint F3 = 1_.2; //~ ERROR: invalid use of underscores in number literal
    uint F4 = 1._2; //~ ERROR: invalid use of underscores in number literal
    uint F5 = 1.2e_12; //~ ERROR: invalid use of underscores in number literal
    uint F6 = 1._; //~ ERROR: empty rational
}
