//! Minimal OSC 1.0 message encoder (int32, float32, string arguments).

#[derive(Debug, Clone, PartialEq)]
pub enum Arg<'a> {
    Int(i32),
    Float(f32),
    Str(&'a str),
}

fn push_padded_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
    // At least one NUL terminator, then pad to a multiple of 4.
    let pad = 4 - (s.len() % 4);
    out.extend(std::iter::repeat_n(0u8, pad));
}

pub fn message(address: &str, args: &[Arg]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    push_padded_str(&mut out, address);
    let mut tags = String::from(",");
    for a in args {
        tags.push(match a {
            Arg::Int(_) => 'i',
            Arg::Float(_) => 'f',
            Arg::Str(_) => 's',
        });
    }
    push_padded_str(&mut out, &tags);
    for a in args {
        match a {
            Arg::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
            Arg::Float(v) => out.extend_from_slice(&v.to_be_bytes()),
            Arg::Str(s) => push_padded_str(&mut out, s),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_message_bytes() {
        let m = message("/forgedmx/beat", &[Arg::Int(1), Arg::Float(128.0)]);
        let mut expected = b"/forgedmx/beat\0\0".to_vec();
        expected.extend_from_slice(b",if\0");
        expected.extend_from_slice(&1i32.to_be_bytes());
        expected.extend_from_slice(&128.0f32.to_be_bytes());
        assert_eq!(m, expected);
    }

    #[test]
    fn sync_message_bytes() {
        let m = message("/forgedmx/sync", &[Arg::Str("api")]);
        let mut expected = b"/forgedmx/sync\0\0".to_vec();
        expected.extend_from_slice(b",s\0\0");
        expected.extend_from_slice(b"api\0");
        assert_eq!(m, expected);
    }

    #[test]
    fn four_byte_aligned_string_gets_full_pad() {
        let m = message("/bpm", &[]);
        assert_eq!(m, b"/bpm\0\0\0\0,\0\0\0".to_vec());
    }
}
