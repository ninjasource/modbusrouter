# modbusrouter
A command line app that routes an incoming TCP stream to a modbus
This tool will connect to the host specified, read the incomming bytes and write to appropriate registers on the locally hosted TCP modbus

Usage: modbusrouter <hostname>
Where hostname is the source of the data

Example: modbusrouter 192.168.1.1:5000

