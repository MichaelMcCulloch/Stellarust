use nom::bytes::complete::take_while;

use std::arch::x86_64::{
    _mm_cmpestri, _mm_loadu_si128, _SIDD_CMP_RANGES, _SIDD_LEAST_SIGNIFICANT, _SIDD_UBYTE_OPS,
};
use std::cmp::min;

//the range of all the characters which should be REJECTED
pub const SPACE_RANGES: &[u8] = &[b'\x00', b'\x08', b'\x0e', b'\x1f', b'!', b'\xff'];

pub const TOKEN_RANGES: &[u8] = &[
    b'\x00', b'\x3c', b'\x3e', b'\x7a', b'\x7c', b'\x7c', b'\x7e', b'\xff',
];
pub const NOT_TOKEN_RANGES: &[u8] = &[b'=', b'=', b'{', b'{', b'}', b'}'];

pub const NUMBER_RANGES: &[u8] = &[b'\x00', b'\x2f', b':', b'\xff'];
pub const ALPHABET_RANGES: &[u8] = &[b'\x00', b'@', b'[', b'`', b'{', b'\xff'];
pub const STRING_LITTERAL_CONTENT_RANGES: &[u8] = &[
    b'\x00', b'\x08', b'\x0e', b'\x1f', b'"', b'"', b'=', b'=', b'{', b'{', b'}', b'}', b'\x7f',
    b'\xff',
];
pub const IDENTIFIER_RANGES: &[u8] = &[
    b'\x00', b'\x2d', b'\x2f', b'\x2f', b':', b'@', b'[', b'^', b'`', b'`', b'{', b'\xff',
];

const SIMD_RANGE: usize = 16;

use nom::error::ParseError;

use super::Res;

pub fn take_while_simd<'a, Condition, Error: ParseError<&'a str>>(
    cond: Condition,
    ranges: &'static [u8],
) -> impl Fn(&'a str) -> Res<&'a str, &'a str>
where
    Condition: Fn(char) -> bool,
{
    // move |i: &'a str| take_while_unrolled(i, |c| !cond(c))
    move |input: &'a str| {
        let condition = |c| cond(c);
        let len = input.len();
        if len == 0 {
            return Ok(("", ""));
        } else if len >= 16 {
            simd_loop16(input, ranges, len)
        } else {
            take_while(condition)(input)
        }
    }
}

fn simd_loop16<'a>(str: &'a str, ranges: &[u8], len: usize) -> Res<&'a str, &'a str> {
    let start = str.as_ptr() as usize;
    let mut i = str.as_ptr() as usize;
    let ranges16 = unsafe { _mm_loadu_si128(ranges.as_ptr() as *const _) };
    let ranges_len = ranges.len() as i32;
    loop {
        let s1 = unsafe { _mm_loadu_si128(i as *const _) };

        let idx = unsafe {
            _mm_cmpestri(
                ranges16,
                ranges_len,
                s1,
                16,
                _SIDD_LEAST_SIGNIFICANT | _SIDD_CMP_RANGES | _SIDD_UBYTE_OPS,
            )
        };

        if idx != 16 {
            i += idx as usize;
            break;
        }
        i += 16;
    }
    let index = i - start;
    let (before, after) = str.split_at(min(index, len));
    return Ok((after, before));
}

#[cfg(test)]
mod tests {
    use nom::error::VerboseError;

    use crate::clausewitz::tables::is_space;

    use super::*;
    #[test]
    fn take_while_simd__string_with_leading_whitespace__whitespace_collected_remainder_returned() {
        let text = " \t\n\r|Stop this is a big long string";
        let ranges = SPACE_RANGES;
        let (remainder, parsed) =
            take_while_simd::<'_, _, VerboseError<&str>>(is_space, ranges)(text).unwrap();
        assert_eq!(remainder, "|Stop this is a big long string");
        assert_eq!(parsed, " \t\n\r");
    }

    #[test]
    fn take_while_simd__string_with_many_leading_whitespace__whitespace_collected_remainder_returned(
    ) {
        let text = "\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t|Stop this is a big long string";
        let ranges = SPACE_RANGES;
        let (remainder, parsed) =
            take_while_simd::<'_, _, VerboseError<&str>>(is_space, ranges)(text).unwrap();
        assert_eq!(remainder, "|Stop this is a big long string");
        assert_eq!(parsed, "\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t");
    }

    #[test]
    fn take_while_simd__short_string__whitespace_collected_remainder_returned() {
        let text = "\t\t\ts";
        let ranges = SPACE_RANGES;
        let (remainder, parsed) =
            take_while_simd::<'_, _, VerboseError<&str>>(is_space, ranges)(text).unwrap();
        assert_eq!(remainder, "s");
        assert_eq!(parsed, "\t\t\t");
    }

    #[test]
    fn take_while_simd__all_white_space__whitespace_collected_remainder_returned() {
        let text = " \t\n\r";
        let ranges = SPACE_RANGES;
        let (remainder, parsed) =
            take_while_simd::<'_, _, VerboseError<&str>>(is_space, ranges)(text).unwrap();
        assert_eq!(remainder, "");
        assert_eq!(parsed, " \t\n\r");
    }
}
