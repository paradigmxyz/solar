contract C {
    enum EnumType {A, B, C}

    struct StructType {
        uint x;
    }
    error E(EnumType StructType, StructType test);
}