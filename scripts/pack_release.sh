#!/bin/bash
cd /home/bitboi/dev_2/Helheim
cargo build --release
zip -j helheim_v1.2.zip target/release/helheim-cli examples/cpu_burn_max.hel examples/matmul_stress.hel
