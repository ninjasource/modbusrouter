use byteorder::{LittleEndian, ReadBytesExt};
use modbus::tcp;
use modbus::{Client, Transport};
use std::cmp::PartialEq;
use std::env;
use std::io;
use std::io::{ErrorKind, Read};
use std::net::TcpStream;

fn main() {
    // hardcode the IP address if one has not been passed in
    let mut host: String = "192.168.1.87:10001".to_string();
    let args: Vec<_> = env::args().collect();
    if args.len() == 2 {
        // 1st param passed into console application
        host = args[1].to_string();
        println!("Parameter host: {} ", host);
    }

    // local modbus connection details
    let cfg = tcp::Config::default();
    let mut modbus_client =
        tcp::Transport::new_with_cfg("127.0.0.1", cfg).expect("Unable to create modbus client");

    // this keeps looping until the program panics
    loop {
        println!("Connecting to {} ...", host);
        let mut stream = TcpStream::connect(&host).expect("Unable to connect to remote host");
        println!("Connected");

        // this keeps looping until an invalid message is encountered or we fail to send the message to the modbus
        // If that happens then the connection will be closed (the stream goes out of scope) and a new connection will be made
        loop {
            // read the message from from the stream
            match read_message(&mut stream) {
                Ok(msg) => {
                    // {:?} automatically prints all the members of the msg
                    println!("Received message #{}: {:?}", msg.msg_num_value, msg);

                    // send the message to the modbus
                    match send_message_to_modbus(msg, &mut modbus_client) {
                        Ok(_) => println!("Successfully sent message to modbus"),
                        Err(e) => {
                            // report an error and break out of the loop
                            // we will disconnect and wait for the next incomming connection
                            eprintln!("Error sending message to modbus: {:?}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    // report an error and break out of the loop
                    // we will disconnect and wait for the next incomming connection
                    eprintln!("Error reading message from host: {:?}", e);
                    break;
                }
            }
        }
    }
}

// This function takes a mutable reference to the stream which implements the Read trait.
// If the read is successful the function will return a populated DeviceMessage struct, otherwise an IO Error
fn read_message<T: Read>(stream: &mut T) -> Result<DeviceMessage, io::Error> {
    // the buffer used to contain a frame of data from the stream
    let mut buffer: [u8; 27] = [0; 27];

    // read until we fill up the buffer
    let mut num_bytes = 0;
    while num_bytes < buffer.len() {
        // pass in a slice of our buffer (we don't want to overwrite what has already been read)
        // the ? is there to propogate OK results or to catch IO errors and exit the function if they are encountered
        num_bytes += stream.read(&mut buffer[num_bytes..])?;
    }

    // check the start sequence is 0x1900
    // slices implement the PartialEq trait so we can call ne function on them (not equal)
    const START_SEQ: [u8; 2] = [0x19, 0x00];
    if buffer[..2].ne(&START_SEQ) {
        let e = io::Error::new(ErrorKind::Other, "Unrecognised start sequence");
        return Err(e);
    }

    // check that the mac address is 0xD0CF5E82937B
    const MAC_ADDRESS: [u8; 6] = [0xD0, 0xCF, 0x5E, 0x82, 0x93, 0x7B];
    if buffer[2..8].ne(&MAC_ADDRESS) {
        let e = io::Error::new(ErrorKind::Other, "Unexpected MAC address");
        return Err(e);
    }

    // check the length field
    if buffer[8] != 0x12 {
        let e = io::Error::new(
            ErrorKind::Other,
            "Length of payload must be 0x12 (18 bytes)",
        );
        return Err(e);
    }

    // read the payload into the DeviceMessage struct
    // we use the byteorder crate with ReadBytesExt extensions to borrowed slices to extract
    // primitive data types out of byte streams. In this case a u16 in LittleEndian byte order
    let message = DeviceMessage {
        batt_pid1: buffer[9],
        batt_value: buffer[10],
        temp_pid2: buffer[11],
        temp_value: buffer[12],
        vib_pid3: buffer[13],
        vib_x: (&buffer[14..16]).read_u16::<LittleEndian>()?,
        vib_y: (&buffer[16..18]).read_u16::<LittleEndian>()?,
        vib_z: (&buffer[18..20]).read_u16::<LittleEndian>()?,
        msg_num_pid5: buffer[20],
        msg_num_value: (&buffer[21..23]).read_u16::<LittleEndian>()?,
        version_pid11: buffer[23],
        version_value: buffer[24],
        rssi_pid6: buffer[25],
        rssi_value: buffer[26],
    };

    // return the message
    Ok(message)
}

// Sends our extracted message to the modbus using the mdbus crate
fn send_message_to_modbus(
    msg: DeviceMessage,
    modbus_client: &mut Transport,
) -> Result<(), modbus::Error> {
    modbus_client.write_single_register(msg.batt_pid1 as u16, msg.batt_value as u16)?;
    modbus_client.write_single_register(msg.temp_pid2 as u16, msg.temp_value as u16)?;

    // sends the vib data in multiple registers starting at pid3
    modbus_client
        .write_multiple_registers(msg.vib_pid3 as u16, &[msg.vib_x, msg.vib_y, msg.vib_z])?;
    modbus_client.write_single_register(msg.msg_num_pid5 as u16, msg.msg_num_value as u16)?;
    modbus_client.write_single_register(msg.version_pid11 as u16, msg.version_value as u16)?;
    modbus_client.write_single_register(msg.rssi_pid6 as u16, msg.rssi_value as u16)?;
    Ok(())
}

// all the useful information extracted from the tcp stream frame
#[derive(Debug)]
struct DeviceMessage {
    batt_pid1: u8,
    batt_value: u8,
    temp_pid2: u8,
    temp_value: u8,
    vib_pid3: u8,
    vib_x: u16,
    vib_y: u16,
    vib_z: u16,
    msg_num_pid5: u8,
    msg_num_value: u16,
    version_pid11: u8,
    version_value: u8,
    rssi_pid6: u8,
    rssi_value: u8,
}

/****************************************************************************************************************/
/*  ****************************************** Tests ************************************************************/
/****************************************************************************************************************/

#[cfg(test)]
mod tests {

    use super::*;
    use std::error::Error;
    use std::io::Cursor;

    #[test]
    fn read_message_multiple_messages() {
        // this byte stream consists of 7 correctly formed messages
        // this test will decode all of them and explicitly check the first two
        let raw = vec![
            0x19, 0x00, 0xD0, 0xCF, 0x5E, 0x82, 0x93, 0x7B, 0x12, 0x01, 0x00, 0x02, 0x54, 0x03,
            0xFE, 0xF2, 0x5A, 0x02, 0x7A, 0x07, 0x05, 0x3A, 0x84, 0x0B, 0x02, 0x06, 0xBD, 0x19,
            0x00, 0xD0, 0xCF, 0x5E, 0x82, 0x93, 0x7B, 0x12, 0x01, 0x00, 0x02, 0x54, 0x03, 0xFF,
            0xF2, 0x77, 0x02, 0x74, 0x07, 0x05, 0x3B, 0x84, 0x0B, 0x02, 0x06, 0xCB, 0x19, 0x00,
            0xD0, 0xCF, 0x5E, 0x82, 0x93, 0x7B, 0x12, 0x01, 0x00, 0x02, 0x54, 0x03, 0xFF, 0xF2,
            0x63, 0x02, 0x76, 0x07, 0x05, 0x3C, 0x84, 0x0B, 0x02, 0x06, 0xC9, 0x19, 0x00, 0xD0,
            0xCF, 0x5E, 0x82, 0x93, 0x7B, 0x12, 0x01, 0x00, 0x02, 0x54, 0x03, 0x15, 0xF3, 0x78,
            0x02, 0x66, 0x07, 0x05, 0x3D, 0x84, 0x0B, 0x02, 0x06, 0xBE, 0x19, 0x00, 0xD0, 0xCF,
            0x5E, 0x82, 0x93, 0x7B, 0x12, 0x01, 0x00, 0x02, 0x54, 0x03, 0x0E, 0xF3, 0x75, 0x02,
            0x38, 0x07, 0x05, 0x3E, 0x84, 0x0B, 0x02, 0x06, 0xCB, 0x19, 0x00, 0xD0, 0xCF, 0x5E,
            0x82, 0x93, 0x7B, 0x12, 0x01, 0x00, 0x02, 0x54, 0x03, 0x07, 0xF3, 0x7B, 0x02, 0x65,
            0x07, 0x05, 0x3F, 0x84, 0x0B, 0x02, 0x06, 0xC9, 0x19, 0x00, 0xD0, 0xCF, 0x5E, 0x82,
            0x93, 0x7B, 0x12, 0x01, 0x00, 0x02, 0x54, 0x03, 0x20, 0xF3, 0x6F, 0x02, 0x5B, 0x07,
            0x05, 0x40, 0x84, 0x0B, 0x02, 0x06, 0xBE,
        ];
        let mut buff = Cursor::new(raw);

        // unwrap will panic if read_message returns an Err
        let msg1 = read_message(&mut buff).unwrap();
        assert_eq!(msg1.batt_pid1, 1);
        assert_eq!(msg1.batt_value, 0);
        assert_eq!(msg1.temp_pid2, 2);
        assert_eq!(msg1.temp_value, 84);
        assert_eq!(msg1.vib_pid3, 3);
        assert_eq!(msg1.vib_x, 62206);
        assert_eq!(msg1.vib_y, 602);
        assert_eq!(msg1.vib_z, 1914);
        assert_eq!(msg1.msg_num_pid5, 5);
        assert_eq!(msg1.msg_num_value, 33850);
        assert_eq!(msg1.version_pid11, 11);
        assert_eq!(msg1.version_value, 2);
        assert_eq!(msg1.rssi_pid6, 6);
        assert_eq!(msg1.rssi_value, 189);

        let msg2 = read_message(&mut buff).unwrap();
        assert_eq!(msg2.batt_pid1, 1);
        assert_eq!(msg2.batt_value, 0);
        assert_eq!(msg2.temp_pid2, 2);
        assert_eq!(msg2.temp_value, 84);
        assert_eq!(msg2.vib_pid3, 3);
        assert_eq!(msg2.vib_x, 62207);
        assert_eq!(msg2.vib_y, 631);
        assert_eq!(msg2.vib_z, 1908);
        assert_eq!(msg2.msg_num_pid5, 5);
        assert_eq!(msg2.msg_num_value, 33851);
        assert_eq!(msg2.version_pid11, 11);
        assert_eq!(msg2.version_value, 2);
        assert_eq!(msg2.rssi_pid6, 6);
        assert_eq!(msg2.rssi_value, 203);

        // read the next 5 messages and ignore the contents
        for _ in 0..5 {
            read_message(&mut buff).unwrap();
        }
    }

    #[test]
    fn read_message_no_start_seq() {
        // this byte strem does not start with the correct start seq (0x19, 0x00)
        let raw = vec![
            0xFF, 0x00, 0xFF, 0xCF, 0x5E, 0x82, 0x93, 0x7B, 0x12, 0x01, 0x00, 0x02, 0x54, 0x03,
            0xFE, 0xF2, 0x5A, 0x02, 0x7A, 0x07, 0x05, 0x3A, 0x84, 0x0B, 0x02, 0x06, 0xBD,
        ];
        let mut buff = Cursor::new(raw);
        let err = read_message(&mut buff).unwrap_err();
        assert_eq!(err.description(), "Unrecognised start sequence");
    }

    #[test]
    fn read_message_unexpected_mac_address() {
        // this byte strem does not start with the correct MAC address (0xD0, 0xCF, 0x5E, 0x82, 0x93, 0x7B)
        let raw = vec![
            0x19, 0x00, 0xFF, 0xCF, 0x5E, 0x82, 0x93, 0x7B, 0x12, 0x01, 0x00, 0x02, 0x54, 0x03,
            0xFE, 0xF2, 0x5A, 0x02, 0x7A, 0x07, 0x05, 0x3A, 0x84, 0x0B, 0x02, 0x06, 0xBD,
        ];
        let mut buff = Cursor::new(raw);
        let err = read_message(&mut buff).unwrap_err();
        assert_eq!(err.description(), "Unexpected MAC address");
    }

    #[test]
    fn read_message_invalid_payload_length() {
        // this byte strem does not start with the correct payload length (0x12)
        let raw = vec![
            0x19, 0x00, 0xD0, 0xCF, 0x5E, 0x82, 0x93, 0x7B, 0xFF, 0x01, 0x00, 0x02, 0x54, 0x03,
            0xFE, 0xF2, 0x5A, 0x02, 0x7A, 0x07, 0x05, 0x3A, 0x84, 0x0B, 0x02, 0x06, 0xBD,
        ];
        let mut buff = Cursor::new(raw);
        let err = read_message(&mut buff).unwrap_err();
        assert_eq!(
            err.description(),
            "Length of payload must be 0x12 (18 bytes)"
        );
    }
}
