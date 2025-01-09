# curved pwm

## Setup

- Dev environment setup: https://github.com/allape/espup/blob/main/READMEE.md
- ```shell
  cp cfg.toml.tpl cfg.toml
  vim cfg.toml # edit it to apply your wifi settings
  
  cd .. && npm run build # build index.html.gz
  
  cd esp32
  
  ./flash.sh # this will take a while if you have bad internet connection
  ```
