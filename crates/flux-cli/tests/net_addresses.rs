//! Address parsing/formatting is independent of network system calls.
extern crate alloc as alloc_crate;
pub use alloc_crate::format;
pub use std::{ffi, fs, io, os};
#[path = "../../../library/std/src/net.rs"]
pub mod facade;
mod support;

fn compare(text: &str) {
    macro_rules! one {
        ($ty:ident) => {{
            let actual = text.parse::<facade::$ty>();
            let expected = text.parse::<std::net::$ty>();
            assert_eq!(
                actual.is_ok(),
                expected.is_ok(),
                "{}: {text:?}",
                stringify!($ty)
            );
            match (actual, expected) {
                (Ok(a), Ok(b)) => {
                    assert_eq!(a.to_string(), b.to_string(), "{text:?}");
                    assert_eq!(format!("{a:?}"), format!("{b:?}"), "{text:?}");
                    assert_eq!(format!("{a:*>48.13}"), format!("{b:*>48.13}"), "{text:?}");
                }
                (Err(a), Err(b)) => {
                    assert_eq!(a.to_string(), b.to_string(), "{text:?}");
                    assert_eq!(format!("{a:?}"), format!("{b:?}"), "{text:?}");
                    assert_eq!(format!("{a:*>48.13}"), format!("{b:*>48.13}"), "{text:?}");
                }
                _ => unreachable!(),
            }
        }};
    }
    one!(Ipv4Addr);
    one!(Ipv6Addr);
    one!(IpAddr);
    one!(SocketAddrV4);
    one!(SocketAddrV6);
    one!(SocketAddr);
}
#[test]
fn address_grammar_and_formatting_match_host_std() {
    for text in [
        "",
        "0.0.0.0",
        "127.0.0.1",
        "255.255.255.255",
        "01.2.3.4",
        "1.02.3.4",
        "1.2.3.4.",
        "1.2.3",
        "1.2.3.4.5",
        "256.0.0.1",
        "-1.2.3.4",
        "+1.2.3.4",
        "0x7f.0.0.1",
        "1.2.3.4 ",
        " 1.2.3.4",
        "::",
        "::1",
        "1::",
        "::ffff:192.0.2.1",
        "::192.0.2.1",
        "1:2:3:4:5:6:192.0.2.1",
        "::ffff:01.2.3.4",
        "1:2:3:4:5:6:7:8",
        "1:2:3:4:5:6:7:8::",
        "1:2:3:4:5:6:7",
        "1::2::3",
        ":::1",
        "1:::2",
        "1:2:3:4:5:6:7:",
        "[::1]",
        "[::1]:80",
        "[::1%42]:80",
        "[::1%0]:0",
        "[::1%4294967295]:65535",
        "[::1%4294967296]:0",
        "[::1%eth0]:1",
        "[::1%]:1",
        "[::1%1%2]:1",
        "[::1]:65536",
        "[::1]:+80",
        "[::1]:-1",
        "[::1]:000000000080",
        "127.0.0.1:000000000080",
        "127.0.0.1:65536",
        "[127.0.0.1]:80",
        "::1:80",
        "🦀",
        "[::1]:１",
        "[::1%１２]:3",
    ] {
        compare(text);
    }
    for mask in 0..256u16 {
        let segments: [u16; 8] = core::array::from_fn(|i| {
            if mask & (1 << i) != 0 {
                0
            } else {
                i as u16 + 1
            }
        });
        let std = std::net::Ipv6Addr::from(segments);
        let ours = facade::Ipv6Addr::from(segments);
        assert_eq!(ours.to_string(), std.to_string());
        compare(&std.to_string());
        compare(&format!("[{}%{}]:{}", std, mask as u32 * 1024, mask));
    }
    let mut random = 0x9c27_38ff_1120_dd29u64;
    for _ in 0..2048 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let v4 = std::net::Ipv4Addr::from(random as u32);
        compare(&v4.to_string());
        compare(&format!("{}:{}", v4, random as u16));
        let bits = (random as u128) << 64 | random.rotate_left(29) as u128;
        let v6 = std::net::Ipv6Addr::from(bits);
        compare(&v6.to_string());
        compare(&format!("[{}%{}]:{}", v6, random as u32, random as u16));
        compare(&v6.to_string().to_uppercase());
    }
    for source in [
        "127.0.0.1",
        "::1",
        "2001:db8::ff00:42:8329",
        "[::ffff:192.0.2.1%14]:65535",
    ] {
        for at in 0..=source.len() {
            for token in [
                ":", ".", "%", "[", "]", " ", "+", "-", "g", "f", "0", "１２",
            ] {
                compare(&format!("{}{}{}", &source[..at], token, &source[at..]));
            }
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn net_values_match_rustc_off_and_on() {
    support::oracle::compare_fixture("net_addresses");
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn enum_constructor_functions_keep_their_nominal_owner() {
    support::oracle::compare_fixture("enum_constructor_symbols");
}
