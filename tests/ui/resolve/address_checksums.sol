contract C {
    // Not OK
    address public a = 0xb71cb1A7ab0B6Bc6c07f5A3Ef2EA36757968A121; //~ ERROR: invalid checksum

    // OK
    address public b = 0xB71cb1A7ab0B6Bc6c07f5A3Ef2EA36757968A121;
}
