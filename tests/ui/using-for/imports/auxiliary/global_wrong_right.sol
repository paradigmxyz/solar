import "./global_wrong_type.sol";

function addRight(WrongSourceInt a, WrongSourceInt b) pure returns (WrongSourceInt) {
    return WrongSourceInt.wrap(WrongSourceInt.unwrap(a) + WrongSourceInt.unwrap(b));
}

function negRight(WrongSourceInt a) pure returns (WrongSourceInt) {
    return WrongSourceInt.wrap(-WrongSourceInt.unwrap(a));
}

using {addRight as +, negRight as -} for WrongSourceInt global;
