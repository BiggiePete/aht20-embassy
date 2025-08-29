use core::cell::RefCell;
use defmt::*;
use embassy_stm32::i2c::{Error, I2c}; // Use Async explicitly
use embassy_stm32::mode::Async;
use embassy_time::{Duration, Timer};

// These might be in your main.rs or lib.rs, ensure they are accessible
// use defmt_rtt as _;
// use panic_probe as _;

const AHT20_ADDRESS: u8 = 0x38; // AHT20 sensor I2C address
const CMD_INITIALIZE: [u8; 3] = [0xBE, 0x08, 0x00]; // Initialization command with calibration enable
const CMD_TRIGGER_MEASUREMENT: [u8; 3] = [0xAC, 0x33, 0x00]; // Trigger measurement command
// const CMD_SOFT_RESET: [u8;1] = [0xBA]; // Soft reset command (optional)

// Delays from datasheet
const DELAY_POWER_ON_MS: u64 = 40; // Sensor needs >20ms after power on, recommend 40ms.
const DELAY_INIT_CALIBRATION_MS: u64 = 80; // Wait for calibration after 0xBE, 0x08, 0x00. Datasheet: 50-80ms.
const DELAY_MEASUREMENT_MS: u64 = 80; // Wait for measurement to complete. Datasheet: >75ms.

#[derive(Debug)]
pub enum Aht20Error {
    I2c(Error),
    NotInitialized,
    MeasurementBusy,
    NotCalibrated,
    // CrcError, // Can be added if CRC check is implemented
}

impl From<Error> for Aht20Error {
    fn from(e: Error) -> Self {
        Aht20Error::I2c(e)
    }
}

pub struct AHT20<'a> {
    i2c: &'a RefCell<I2c<'static, Async>>,
    initialized: bool, // Track initialization state
}

impl<'a> AHT20<'a> {
    /// Creates a new AHT20 driver.
    /// It's recommended to call `init()` after this.
    pub fn new(i2c: &'a RefCell<I2c<'static, Async>>) -> Self {
        AHT20 {
            i2c,
            initialized: false,
        }
    }

    /// Initializes the AHT20 sensor.
    /// This should be called once after creating the sensor instance or after a soft reset.
    /// It ensures the sensor is calibrated.
    pub async fn init(&mut self) -> Result<(), Aht20Error> {
        // A small delay after power-on if this is the very first communication
        // Timer::after(Duration::from_millis(DELAY_POWER_ON_MS)).await; // Often handled by system startup

        self.i2c
            .borrow_mut()
            .write(AHT20_ADDRESS, &CMD_INITIALIZE)
            .await
            .map_err(|e| {
                error!("AHT20: I2C Init Write Error: {:?}", e);
                Aht20Error::I2c(e)
            })?;

        Timer::after(Duration::from_millis(DELAY_INIT_CALIBRATION_MS)).await;

        // Verify initialization by reading status
        let mut status_byte = [0u8; 1];
        self.i2c
            .borrow_mut()
            .read(AHT20_ADDRESS, &mut status_byte)
            .await
            .map_err(|e| {
                error!("AHT20: I2C Read Status after Init Error: {:?}", e);
                Aht20Error::I2c(e)
            })?;

        if (status_byte[0] & 0x08) == 0 {
            // Bit 3 is CalEnable
            error!("AHT20: Sensor failed to calibrate after init command.");
            self.initialized = false;
            return Err(Aht20Error::NotCalibrated);
        }
        if (status_byte[0] & 0x80) != 0 {
            // Bit 7 is Busy
            error!("AHT20: Sensor busy after init command (unexpected).");
            // This is unlikely but good to check
        }

        info!("AHT20: Initialized and calibrated successfully.");
        self.initialized = true;
        Ok(())
    }

    /// Reads temperature (in Celsius) and relative humidity (in %).
    /// Ensures the sensor is initialized before reading.
    pub async fn read(&mut self) -> Result<(f32, f32), Aht20Error> {
        if !self.initialized {
            warn!("AHT20: Sensor not initialized. Attempting to initialize now.");
            // Attempt to initialize if not already done.
            // Alternatively, return Aht20Error::NotInitialized and require user to call init().
            self.init().await.map_err(|e| {
                error!("AHT20: Failed to auto-initialize sensor");
                e
            })?;
        }

        // Trigger measurement
        self.i2c
            .borrow_mut()
            .write(AHT20_ADDRESS, &CMD_TRIGGER_MEASUREMENT)
            .await
            .map_err(|e| {
                error!("AHT20: I2C Trigger Measurement Error: {:?}", e);
                Aht20Error::I2c(e)
            })?;

        // Wait for the measurement to complete
        Timer::after(Duration::from_millis(DELAY_MEASUREMENT_MS)).await;

        // Read the sensor data (7 bytes: Status, H, H, H/T, T, T, CRC)
        let mut data = [0u8; 7];
        self.i2c
            .borrow_mut()
            .read(AHT20_ADDRESS, &mut data)
            .await
            .map_err(|e| {
                error!("AHT20: I2C Read Data Error: {:?}", e);
                Aht20Error::I2c(e)
            })?;

        // Check status byte
        // Bit 7 (Busy flag): 0 indicates measurement complete, 1 indicates busy.
        if (data[0] & 0x80) != 0 {
            error!(
                "AHT20: Measurement data not ready (sensor busy). Status: {=u8:08b}",
                data[0]
            );
            return Err(Aht20Error::MeasurementBusy);
        }
        // Bit 3 (CalEnable): Should be 1 if calibrated.
        if (data[0] & 0x08) == 0 {
            warn!(
                "AHT20: Sensor indicates not calibrated! Readings might be inaccurate. Status: {=u8:08b}",
                data[0]
            );
            // This could mean init was skipped or failed. Mark as uninitialized.
            self.initialized = false;
            return Err(Aht20Error::NotCalibrated);
        }
        // Other bits: Bit 6-5 Factory reserved (00), Bit 4 Reserved (0), Bit 2-0 Reserved (000)
        // Bit 0 is also related to CRC in some interpretations, but CRC is on byte 6.

        // Parse the data according to AHT20 datasheet
        // Status: data[0]
        // RH: data[1], data[2], data[3] bits 7:4
        // Temp: data[3] bits 3:0, data[4], data[5]
        // CRC: data[6] (not currently checked)

        let raw_hum = ((data[1] as u32) << 12) | ((data[2] as u32) << 4) | ((data[3] as u32) >> 4);

        let raw_temp =
            (((data[3] as u32) & 0x0F) << 16) | ((data[4] as u32) << 8) | (data[5] as u32);

        // Convert raw values to actual temperature and humidity
        // Formulas from AHT20 datasheet:
        // Humidity (%) = (S_RH / 2^20) * 100%
        // Temperature (°C) = (S_T / 2^20) * 200 - 50
        let humidity = (raw_hum as f32 / 1_048_576.0) * 100.0;
        let temperature = (raw_temp as f32 / 1_048_576.0) * 200.0 - 50.0;

        Ok((temperature, humidity))
    }

    // Optional: Soft reset
    // pub async fn soft_reset(&mut self) -> Result<(), Aht20Error> {
    //     self.i2c
    //         .borrow_mut()
    //         .write(AHT20_ADDRESS, &CMD_SOFT_RESET)
    //         .await
    //         .map_err(|e| {
    //             error!("AHT20: I2C Soft Reset Error: {:?}", e);
    //             Aht20Error::I2c(e)
    //         })?;
    //     self.initialized = false; // Requires re-initialization
    //     Timer::after(Duration::from_millis(DELAY_POWER_ON_MS)).await; // Sensor needs time after reset
    //     Ok(())
    // }
}
