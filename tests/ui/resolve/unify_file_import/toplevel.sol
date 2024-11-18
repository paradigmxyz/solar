// OK
import "./auxiliary/file1.sol";
import "./auxiliary/file2.sol";
import "./auxiliary/imported.sol" as Imported;
import "./auxiliary/imported.sol" as Imported;

//~v ERROR: identifier `Imported` already declared
import "./auxiliary/different_imported.sol" as Imported;
