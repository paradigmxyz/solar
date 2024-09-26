contract DollarIdentifiers {
    struct $dStruct {
        uint256 $dField;
    }

    enum $dEnum {
        $dVariant
    }

    type $dUDVT is uint256;

    $dStruct public $dPublicVariable;

    function $dFunction($dStruct memory $dStructArg, $dEnum $dEnumArg, $dUDVT $dUDVTArg) external {}
}
