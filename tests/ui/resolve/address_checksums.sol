contract C {
    // Not OK
    address public a = 0xb71cb1A7ab0B6Bc6c07f5A3Ef2EA36757968A121; //~ ERROR: invalid checksum
    address public c = 0xb71c_b1A7_ab0B_6Bc6_c07f_5A3E_f2EA_3675_7968_A121; //~ ERROR: invalid checksum

    // OK
    address public b = 0xB71cb1A7ab0B6Bc6c07f5A3Ef2EA36757968A121;
    address public d = 0xB71c_b1A7_ab0B_6Bc6_c07f_5A3E_f2EA_3675_7968_A121;
}
