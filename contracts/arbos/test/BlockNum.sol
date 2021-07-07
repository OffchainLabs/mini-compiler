pragma solidity >=0.4.21 <0.7.0;

contract BlockNum {
    uint public currBlock;

    function setBlock() public {
        currBlock = getBlock();
    }

    function getBlock() public view returns (uint) {
        return block.number;
    }
}
