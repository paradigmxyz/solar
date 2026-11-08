contract C {
    bool public simple;
    bool[] public array;
    mapping(string k => bool v) public map;
    mapping(string k => bool[] v) public mapOfArrays;
    mapping(string k1 => mapping(string k2 => bool v2) v1) public nestedMap;
    mapping(string k1 => mapping(string k2 => bool[] v2) v1) public nestedMapOfArrays;
    mapping(string k1 => mapping(string k2 => bool v2)[] v1) public nestedArrayOfMaps;
    mapping(string k1 => mapping(string k2 => bool[] v2)[] v1) public nestedArrayOfMapsOfArrays;

    function referenceNames() public view {
        simple;
        array;
        map;
        mapOfArrays;
        nestedMap;
        nestedMapOfArrays;
        nestedArrayOfMaps;
        nestedArrayOfMapsOfArrays;
    }

    function referenceThis() public view {
        this.simple; //~ WARN: statement has no effect
        this.array; //~ WARN: statement has no effect
        this.map; //~ WARN: statement has no effect
        this.mapOfArrays; //~ WARN: statement has no effect
        this.nestedMap; //~ WARN: statement has no effect
        this.nestedMapOfArrays; //~ WARN: statement has no effect
        this.nestedArrayOfMaps; //~ WARN: statement has no effect
        this.nestedArrayOfMapsOfArrays; //~ WARN: statement has no effect
    }

    function doCall() public view {
        bool x1 = this.simple();
        bool x2 = this.array(0);
        bool x3 = this.map("");
        bool x4 = this.mapOfArrays("", 0);
        bool x5 = this.nestedMap("", "");
        bool x6 = this.nestedMapOfArrays("", "", 0);
        bool x7 = this.nestedArrayOfMaps("", 0, "");
        bool x8 = this.nestedArrayOfMapsOfArrays("", 0, "", 0);
    }
}
