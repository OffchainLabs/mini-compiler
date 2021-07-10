pragma solidity >=0.4.21 <0.7.0;

import "./SelfDestructor.sol";


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

        uint timestamp = block.timestamp;
        if ((timestamp < 10000000) || (timestamp > 10001000)) {
            emit TestFail(2, 0);
        }

        if (address(this).balance > 0) {
            emit TestFail(3, 0);
        }

        if (tx.origin != address(0)) {
            emit TestFail(4, 0);
        }

        uint reread;
        assembly {
            mstore8(50000, 73)
            reread := mload(49969)
        }
        if (reread != 73) {
            emit TestFail(5, reread);
        }

        uint memSize;
        assembly {
            memSize := msize()
        }
        if (memSize != 50016) {
            emit TestFail(6, memSize);
        }
    }

    function makeLog0() public {
        assembly {
            mstore8(50000, 73)
            log0(49969, 32)
        }
    }

    function destructTest(address victim) public {
        uint bal = address(this).balance;
        SelfDestructor(victim).die(address(this));
        if ((address(this).balance - bal) != 777) {
            emit TestFail(7, address(this).balance-bal);
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
