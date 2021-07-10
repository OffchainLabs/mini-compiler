pragma solidity >=0.4.21 <0.7.0;


contract SelfDestructor {
    function die(address recipient) public {
        assembly {
            selfdestruct(recipient)
        }
    }
}
