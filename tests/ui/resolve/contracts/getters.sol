contract C {
    bool public simple;
    bool[] public array;
    mapping(string k => bool v) public map;
    mapping(string k => bool[] v) public mapOfArrays;
    mapping(string k1 => mapping(string k2 => bool v2) v1) public nestedMap;
    mapping(string k1 => mapping(string k2 => bool[] v2) v1) public nestedMapOfArrays;
    mapping(string k1 => mapping(string k2 => bool v2)[] v1) public nestedArrayOfMaps;
    mapping(string k1 => mapping(string k2 => bool[] v2)[] v1) public nestedArrayOfMapsOfArrays;

    function referenceNames() public {
        simple;
        array;
        map;
        mapOfArrays;
        nestedMap;
        nestedMapOfArrays;
        nestedArrayOfMaps;
        nestedArrayOfMapsOfArrays;
    }

    function referenceThis() public {
        this.simple;
        this.array;
        this.map;
        this.mapOfArrays;
        this.nestedMap;
        this.nestedMapOfArrays;
        this.nestedArrayOfMaps;
        this.nestedArrayOfMapsOfArrays;
    }

    function doCall() public {
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
