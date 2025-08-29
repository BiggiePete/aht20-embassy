
# aht20-embassy

**Embassy compatible, No Std AHT20 library. Designed to be simple and easy to use, without moving the I2C object**

A minimal, no_std, async AHT20 temperature and humidity sensor driver for [Embassy](https://embassy.dev/) on STM32 microcontrollers.  
This library is designed to be simple, ergonomic, and efficient, without taking ownership of the I2C peripheral.

## Features

- Async/await support using Embassy
- No_std compatible
- Does not take ownership of the I2C bus (works with shared/borrowed I2C)
- Simple API for initialization and reading sensor data
- Defmt logging support

## Requirements

- Rust embedded toolchain
- Embassy STM32 HAL
- AHT20 sensor connected via I2C

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
aht20-embassy = { git = "https://github.com/BiggiePete/aht20-embassy" }
# ...other dependencies as shown in this repo's Cargo.toml
```

## Usage

### 1. Create the I2C peripheral

Set up your I2C peripheral using Embassy STM32 HAL as usual.  
Wrap it in a `RefCell` to allow shared mutability.

```rust
use embassy_stm32::i2c::I2c;
use embassy_stm32::mode::Async;
use core::cell::RefCell;

// ... I2C peripheral setup code ...
let i2c = RefCell::new(I2c::new(/* ... */));
```

### 2. Create and initialize the sensor

```rust
use aht20_embassy::AHT20;

let mut sensor = AHT20::new(&i2c);
sensor.init().await.unwrap();
```

### 3. Read temperature and humidity

```rust
let (temperature, humidity) = sensor.read().await.unwrap();
defmt::info!("Temperature: {}°C, Humidity: {}%", temperature, humidity);
```

### 4. Error handling

All methods return a `Result<T, Aht20Error>`.  
Errors include I2C errors, not initialized, sensor busy, or not calibrated.

```rust
match sensor.read().await {
 Ok((temp, hum)) => { /* use values */ }
 Err(e) => defmt::error!("Sensor error: {:?}", e),
}
```

## Example

```rust
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
 // ... I2C setup ...
 let i2c = RefCell::new(I2c::new(/* ... */));
 let mut sensor = AHT20::new(&i2c);

 sensor.init().await.unwrap();

 loop {
  match sensor.read().await {
   Ok((temp, hum)) => {
    defmt::info!("Temp: {:.2}°C, Humidity: {:.2}%", temp, hum);
   }
   Err(e) => {
    defmt::error!("AHT20 error: {:?}", e);
   }
  }
  embassy_time::Timer::after(embassy_time::Duration::from_secs(2)).await;
 }
}
```

## License

MIT © 2025 Peter C
