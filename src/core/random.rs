use windows_sys::Win32::Security::Cryptography::{
    BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};

pub fn bytes(count: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; count];
    if count == 0 {
        return buffer;
    }
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buffer.as_mut_ptr(),
            count as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        fallback(&mut buffer);
    }
    buffer
}

pub fn hex(byte_count: usize) -> String {
    bytes(byte_count)
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect()
}

pub fn guid() -> String {
    let value = hex(16);
    format!(
        "{}-{}-{}-{}-{}",
        &value[0..8],
        &value[8..12],
        &value[12..16],
        &value[16..20],
        &value[20..32]
    )
}

fn fallback(buffer: &mut [u8]) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = nanos
        .rotate_left(17)
        .wrapping_mul(0x2545_F491_4F6C_DD1D)
        .wrapping_add(std::process::id() as u64);
    for slot in buffer.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *slot = (state & 0xFF) as u8;
    }
}
