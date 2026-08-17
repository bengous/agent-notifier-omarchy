use std::fmt::Write as _;
use std::fs;
use std::io::Read;

use crate::event::store::current_millis;

pub(in crate::intake) fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0_u8; bytes];
    if fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut buffer))
        .is_err()
    {
        let fallback = current_millis().to_le_bytes();
        for (index, byte) in buffer.iter_mut().enumerate() {
            *byte = fallback[index % fallback.len()];
        }
    }
    buffer
        .iter()
        .fold(String::with_capacity(bytes * 2), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        })
}
