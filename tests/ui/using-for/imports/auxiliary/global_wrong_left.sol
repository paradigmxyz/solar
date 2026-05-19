import "./global_wrong_type.sol";

function addLeft(WrongSourceInt a, WrongSourceInt b) pure returns (WrongSourceInt) {
    return WrongSourceInt.wrap(WrongSourceInt.unwrap(a) + WrongSourceInt.unwrap(b));
}

function negLeft(WrongSourceInt a) pure returns (WrongSourceInt) {
    return WrongSourceInt.wrap(-WrongSourceInt.unwrap(a));
}

using {addLeft as +, negLeft as -} for WrongSourceInt global;
