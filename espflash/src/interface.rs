//! Serial port wrapper to support platform-specific functionality
//!
//! Since we support flashing using a Raspberry Pi's built-in UART, we must be
//! able to abstract over the differences between this setup and when using a
//! serial port as one normally would, ie.) via USB.

use std::io::Read;

use miette::{Context, Result};
#[cfg(feature = "raspberry")]
use rppal::gpio::{Gpio, OutputPin};
use serialport::{FlowControl, SerialPort, SerialPortInfo};

use crate::error::Error;

/// Errors relating to the configuration of a serial port
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum SerialConfigError {
    #[cfg(feature = "raspberry")]
    #[error("You need to specify both DTR and RTS pins when using an internal UART peripheral")]
    MissingDtrRtsForInternalUart,
    #[cfg(feature = "raspberry")]
    #[error("GPIO {0} is not available")]
    GpioUnavailable(u8),
}

/// Wrapper around SerialPort where platform-specific modifications can be
/// implemented.
pub struct Interface {
    /// Hardware serial port used for communication
    pub serial_port: Box<dyn SerialPort>,
    /// Data Transmit Ready pin
    #[cfg(feature = "raspberry")]
    pub dtr: Option<OutputPin>,
    /// Ready To Send pin
    #[cfg(feature = "raspberry")]
    pub rts: Option<OutputPin>,
}

/// Set the level of a GPIO
#[cfg(feature = "raspberry")]
fn write_gpio(gpio: &mut OutputPin, level: bool) {
    if level {
        gpio.set_high();
    } else {
        gpio.set_low();
    }
}

/// Open a serial port
fn open_port(port_info: &SerialPortInfo) -> Result<Box<dyn SerialPort>> {
    serialport::new(&port_info.port_name, 115_200)
        .flow_control(FlowControl::None)
        .open()
        .map_err(Error::from)
        .wrap_err_with(|| format!("Failed to open serial port {}", port_info.port_name))
}

impl Interface {
    #[cfg(feature = "raspberry")]
    pub fn new(port_info: &SerialPortInfo, dtr: Option<u8>, rts: Option<u8>) -> Result<Self> {
        if port_info.port_type == serialport::SerialPortType::Unknown
            && (dtr.is_none() || rts.is_none())
        {
            // Assume internal UART, which has no DTR pin and usually no RTS either.
            return Err(Error::from(SerialConfigError::MissingDtrRtsForInternalUart).into());
        }

        let gpios = Gpio::new().unwrap();

        let rts = if let Some(gpio) = rts {
            match gpios.get(gpio) {
                Ok(pin) => Some(pin.into_output()),
                Err(_) => return Err(Error::from(SerialConfigError::GpioUnavailable(gpio)).into()),
            }
        } else {
            None
        };

        let dtr = if let Some(gpio) = dtr {
            match gpios.get(gpio) {
                Ok(pin) => Some(pin.into_output()),
                Err(_) => return Err(Error::from(SerialConfigError::GpioUnavailable(gpio)).into()),
            }
        } else {
            None
        };

        Ok(Self {
            serial_port: open_port(port_info)?,
            rts,
            dtr,
        })
    }

    #[cfg(not(feature = "raspberry"))]
    pub fn new(port_info: &SerialPortInfo, _dtr: Option<u8>, _rts: Option<u8>) -> Result<Self> {
        Ok(Self {
            serial_port: open_port(port_info)?,
        })
    }

    /// Set the level of the DTR pin
    pub fn write_data_terminal_ready(&mut self, pin_state: bool) -> serialport::Result<()> {
        #[cfg(feature = "raspberry")]
        if let Some(gpio) = self.dtr.as_mut() {
            write_gpio(gpio, pin_state);
            return Ok(());
        }

        self.serial_port.write_data_terminal_ready(pin_state)
    }

    /// Set the level of the RTS pin
    pub fn write_request_to_send(&mut self, pin_state: bool) -> serialport::Result<()> {
        #[cfg(feature = "raspberry")]
        if let Some(gpio) = self.rts.as_mut() {
            write_gpio(gpio, pin_state);
            return Ok(());
        }

        self.serial_port.write_request_to_send(pin_state)
    }

    /// Turn an [Interface] into a [SerialPort]
    pub fn into_serial(self) -> Box<dyn SerialPort> {
        self.serial_port
    }

    /// Turn an [Interface] into a `&`[SerialPort]
    pub fn serial_port(&self) -> &dyn SerialPort {
        self.serial_port.as_ref()
    }

    /// Turn an [Interface] into a  `&mut `[SerialPort]
    pub fn serial_port_mut(&mut self) -> &mut dyn SerialPort {
        self.serial_port.as_mut()
    }
}

// Note(dbuga): this `impl` is necessary because using `dyn SerialPort` as `dyn
// Read` requires trait_upcasting which isn't stable yet.
impl Read for Interface {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.serial_port.read(buf)
    }
}
