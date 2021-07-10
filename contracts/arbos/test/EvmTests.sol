pragma solidity >=0.4.21 <0.7.0;


contract EvmTests {
    function test(address otherAddr) public {
        uint a = 1;
        uint b = 1;
        if ((a+b) != 2) {
            emit TestFail(0, 0);
        }

        bytes memory otherCode = getCode(otherAddr);
        bytes32 otherHash = keccak256(abi.encodePacked(otherCode));
        if (otherHash != getCodeHash(otherAddr)) {
            emit TestFail(1, 0);
        }
    }

    function getCode(address addr) internal returns(bytes memory o_code) {
        assembly {
        // retrieve the size of the code, this needs assembly
            let size := extcodesize(addr)
        // allocate output byte array - this could also be done without assembly
        // by using o_code = new bytes(size)
            o_code := mload(0x40)
        // new "memory end" including padding
            mstore(0x40, add(o_code, and(add(add(size, 0x20), 0x1f), not(0x1f))))
        // store length in memory
            mstore(o_code, size)
        // actually retrieve the code, this needs assembly
            extcodecopy(addr, add(o_code, 0x20), 0, size)
        }
    }

    function getCodeHash(address addr) internal returns(bytes32 o_hash) {
        assembly {
            o_hash := extcodehash(addr)
        }
    }

    event TestFail(uint code1, uint code2);
}
