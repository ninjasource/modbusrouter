# modbusrouter
A command line app that routes an incoming TCP stream to a modbus
This tool will connect to the host specified, read the incoming bytes and write to appropriate registers on the locally hosted TCP modbus

Usage: modbusrouter hostname

Where hostname is the source of the data

Example: 
```
modbusrouter 192.168.1.1:5000
```

There are two main functions:
1. read_message - This reads bytes from the TCP stream into a DeviceMessage struct
2. send_message_to_modbus - This writes the DeviceMessage to the modbus

Here is the pseudo code for the main function:

```rust
loop {
    stream = connect()
    loop {
        msg = read_message(stream)
        send_message_to_modbus(msg)
    }
}
```

If either the `read_message()` or the `send_message_to_modbus()` functions fail then the program exists then inner loop and closes the tcp connection (not the modbus connection). The outer loop ensures that a new TCP connection will then be attempted. It is assumed that the host will send a correctly formated message when a new connection is initiated and not simply continue to send bytes from the last position it originally sent from. If this were the case we would have to search for magic byte strings to synchronise the client and server.



