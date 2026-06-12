//@compile-flags: -Zcodegen --emit=mir

contract ConstructorAbiValidation {
    bool public flag;
    bool public second;

    constructor(bool flag_, bool[2] memory flags) {
        flag = flag_;
        second = flags[1];
    }
}
