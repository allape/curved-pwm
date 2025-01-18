# Curved PWM

Edit PWM steps with a curve editor.

- Fan speed control with a temperature curve.
- Flash pattern with a brightness curve.
- Loaded rotor with a torque curve.

### ESP32

- [esp32](esp32/README.md) in rust
    - esp32c3 for now

### Credits

- [Favicon](esp32/src/fan.png): https://www.irasutoya.com/2019/07/blog-post_8.html
- Rotating Image: https://www.irasutoya.com/2017/09/blog-post_987.html
- ESP32: https://docs.espressif.com/projects/esp-idf/en/v5.4/esp32c3/get-started/index.html
- ESPUP: https://github.com/esp-rs/espup

### What I learned

- Language `Rust`
- `ESP-IDF`
- When put a `&str.as_prt()` into C function, it must be ended with `\0`.
  - Otherwise, the `invalid memory access` will occur, and there will be no error message.
